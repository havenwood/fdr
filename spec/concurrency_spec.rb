# frozen_string_literal: true

require_relative "spec_helper"
require "fileutils"
require "open3"
require "timeout"
require "tmpdir"

describe "Fdr concurrency" do
  before do
    @dir = Dir.mktmpdir("fdr-concurrency")
    10.times do |i|
      subdir = File.join(@dir, "dir#{i}")
      Dir.mkdir(subdir)
      20.times { |j| File.write(File.join(subdir, "file#{j}.txt"), "needle\n") }
    end
  end

  after do
    FileUtils.remove_entry(@dir)
  end

  # Retry in case a fast search finishes before the ticker starts.
  def ticks_during
    5.times do
      ticks = 0
      thread = Thread.new { loop { ticks += 1 } }
      sleep 0.01 while ticks.zero?
      before = ticks
      yield
      during = ticks - before
      thread.kill
      thread.join
      return during if during.positive?
    end

    0
  end

  def scheduler_with_worker_pool
    Class.new do
      attr_reader :blocking_operations

      def initialize
        @blocking_operations = 0
      end

      def block(*) = false
      def unblock(*) = nil
      def kernel_sleep(*) = 0
      def io_wait(*) = 0
      def close = nil

      def fiber(&block)
        Fiber.new(blocking: false, &block).tap(&:resume)
      end

      def fiber_interrupt(fiber, exception)
        fiber.raise(exception)
      end

      def blocking_operation_wait(operation)
        @blocking_operations += 1
        worker = Thread.new { operation.call }
        Thread.pass while worker.alive?
        worker.value
      end
    end.new
  end

  it "releases the GVL so other threads run during search" do
    during = ticks_during { Fdr.search(paths: [@dir], hidden: true) }
    assert_predicate during, :positive?, "other threads should run during Fdr.search"
  end

  it "releases the GVL so other threads run during grep" do
    during = ticks_during { Fdr.grep(pattern: "needle", paths: [@dir]) }
    assert_predicate during, :positive?, "other threads should run during Fdr.grep"
  end

  it "offloads waits through Ruby 3.4's scheduler hook" do
    skip "requires Ruby 3.4 or newer" if Gem::Version.new(RUBY_VERSION) < Gem::Version.new("3.4")

    scheduler = scheduler_with_worker_pool
    Fiber.set_scheduler(scheduler)
    results = nil
    Fiber.schedule { results = Fdr.search(paths: [@dir], type: "f") }
    Fiber.set_scheduler(nil)

    assert_equal 200, results.length
    assert_predicate scheduler.blocking_operations, :positive?
  ensure
    Fiber.set_scheduler(nil) if scheduler && Fiber.scheduler.equal?(scheduler)
  end

  it "can be interrupted by Timeout during search" do
    assert_raises(Timeout::Error) do
      Timeout.timeout(0.001) { 50.times { Fdr.search(paths: [@dir], hidden: true) } }
    end
  end

  it "can be interrupted by Timeout while grepping a large file" do
    path = File.join(@dir, "large.txt")
    chunk = "haystack\n" * (1024 * 1024 / 9)
    File.open(path, "wb") { |file| 16.times { file.write(chunk) } }
    slow_no_match = "(?i:(?:ha|hay|hays|haystac)+z)"

    assert_raises(Timeout::Error) do
      Timeout.timeout(0.01) { Fdr.grep(pattern: slow_no_match, paths: [path]) }
    end
  end

  it "searches in a forked child after the parent has searched" do
    skip "fork is unavailable" unless Process.respond_to?(:fork)

    Fdr.search(paths: [@dir], hidden: true)
    pid = fork do
      Fdr.search(paths: [@dir], hidden: true)
      Fdr.grep(pattern: "needle", paths: [@dir])
      exit 0
    end
    _, status = Process.waitpid2(pid)

    assert_predicate status, :success?, "forked child should search, got #{status.inspect}"
  end

  it "completes despite spurious thread wakeups" do
    thread = Thread.new do
      Fdr.search(paths: [@dir], hidden: true)
    rescue => e
      e
    end

    begin
      thread.wakeup while thread.alive?
    rescue ThreadError
    end

    assert_kind_of Array, thread.value, "spurious wakeups should not abort the search"
  end

  it "delivers a timeout despite wakeup pressure" do
    paths = Array.new(1_000, @dir)
    thread = Thread.new do
      Timeout.timeout(0.04) { Fdr.search(paths:, hidden: true) }
      :completed
    rescue Timeout::Error
      :timed_out
    end

    begin
      thread.wakeup while thread.alive?
    rescue ThreadError
    end

    assert_equal :timed_out, thread.value, "wakeup pressure should not swallow a timeout"
  end

  it "completes grep despite spurious thread wakeups" do
    thread = Thread.new do
      Fdr.grep(pattern: "needle", paths: [@dir])
    rescue => e
      e
    end

    begin
      thread.wakeup while thread.alive?
    rescue ThreadError
    end

    assert_kind_of Hash, thread.value, "spurious wakeups should not abort the grep"
  end

  # Run in a child with YJIT off because it skips Ractor checks for C calls.
  it "searches and greps from Ractors" do
    script = <<~RUBY
      require "fdr"
      Warning[:experimental] = false
      Thread.report_on_exception = false

      ractors = Array.new(4) do
        Ractor.new(ARGV[0]) do |path|
          search = Fdr.search(pattern: "file1", paths: [path], extension: "txt")
          grep = Fdr.grep(pattern: "needle", paths: [path])
          invalid_pattern = begin
            Fdr.search(pattern: "[", paths: [path])
            false
          rescue Fdr::InvalidPattern
            true
          end
          [search.length, grep.length, invalid_pattern]
        end
      end

      print ractors.map { |r| r.respond_to?(:value) ? r.value : r.take }.inspect
    RUBY
    output, status = Open3.capture2(
      {"RUBYOPT" => nil, "RUBYLIB" => nil, "RUBY_YJIT_ENABLE" => nil, "RUBY_ZJIT_ENABLE" => nil},
      Gem.ruby,
      "--disable-yjit",
      "-I#{File.expand_path("../lib", __dir__)}",
      "-e",
      script,
      @dir
    )

    assert_predicate status, :success?
    assert_equal ([[110, 200, true]] * 4).inspect, output
  end
end
