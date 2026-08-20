# frozen_string_literal: true

require_relative 'spec_helper'
require 'fileutils'
require 'timeout'
require 'tmpdir'

describe 'Fdr concurrency' do
  before do
    @dir = Dir.mktmpdir('fdr-concurrency')
    20.times do |i|
      subdir = File.join(@dir, "dir#{i}")
      Dir.mkdir(subdir)
      100.times { |j| File.write(File.join(subdir, "file#{j}.txt"), "needle\n") }
    end
  end

  after do
    FileUtils.remove_entry(@dir)
  end

  def ticks_during
    ticks = 0
    thread = Thread.new { loop { ticks += 1 } }
    sleep 0.01 while ticks.zero?
    before = ticks
    yield
    during = ticks - before
    thread.kill
    thread.join
    during
  end

  it 'releases the GVL so other threads run during search' do
    during = ticks_during { Fdr.search(paths: [@dir], hidden: true) }
    assert_predicate during, :positive?, 'other threads should run during Fdr.search'
  end

  it 'releases the GVL so other threads run during grep' do
    during = ticks_during { Fdr.grep(pattern: 'needle', paths: [@dir]) }
    assert_predicate during, :positive?, 'other threads should run during Fdr.grep'
  end

  it 'can be interrupted by Timeout during search' do
    assert_raises(Timeout::Error) do
      Timeout.timeout(0.001) { 50.times { Fdr.search(paths: [@dir], hidden: true) } }
    end
  end

  it 'completes despite spurious thread wakeups' do
    thread = Thread.new do
      Fdr.search(paths: [@dir], hidden: true)
    rescue StandardError => e
      e
    end

    begin
      thread.wakeup while thread.alive?
    rescue ThreadError
      # The thread finished between the alive? check and the wakeup.
    end

    assert_kind_of Array, thread.value, 'spurious wakeups should not abort the search'
  end
end
