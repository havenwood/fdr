# frozen_string_literal: true

require_relative "spec_helper"
require "fileutils"
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

  it "releases the GVL so other threads run during search" do
    during = ticks_during { Fdr.search(paths: [@dir], hidden: true) }
    assert_predicate during, :positive?, "other threads should run during Fdr.search"
  end

  it "releases the GVL so other threads run during grep" do
    during = ticks_during { Fdr.grep(pattern: "needle", paths: [@dir]) }
    assert_predicate during, :positive?, "other threads should run during Fdr.grep"
  end
end
