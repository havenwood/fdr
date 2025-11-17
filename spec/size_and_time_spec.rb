# frozen_string_literal: true

require_relative 'spec_helper'
require 'tmpdir'
require 'fileutils'

describe 'Size and time filtering' do
  describe 'size filtering' do
    before do
      @tmpdir = Dir.mktmpdir('fdr_size_test')
      @small_file = File.join(@tmpdir, 'small.txt')
      @medium_file = File.join(@tmpdir, 'medium.txt')
      @large_file = File.join(@tmpdir, 'large.txt')

      File.write(@small_file, 'x' * 50)
      File.write(@medium_file, 'x' * 500)
      File.write(@large_file, 'x' * 5000)
    end

    after do
      FileUtils.rm_rf(@tmpdir) if @tmpdir && File.exist?(@tmpdir)
    end

    it 'filters files by min_size' do
      results = Fdr.search(min_size: 100, paths: [@tmpdir], type: 'f')

      assert(results.any? { |p| p.include?('medium.txt') },
             'should find file >= min_size')
      assert(results.any? { |p| p.include?('large.txt') },
             'should find larger files')
      refute(results.any? { |p| p.include?('small.txt') },
             'should not find file < min_size')
    end

    it 'filters files by max_size' do
      results = Fdr.search(max_size: 1000, paths: [@tmpdir], type: 'f')

      assert(results.any? { |p| p.include?('small.txt') },
             'should find file <= max_size')
      assert(results.any? { |p| p.include?('medium.txt') },
             'should find file <= max_size')
      refute(results.any? { |p| p.include?('large.txt') },
             'should not find file > max_size')
    end

    it 'combines size and time filters' do
      results = Fdr.search(
        min_size: 100,
        max_size: 1_000_000,
        changed_within: 86_400 * 365,
        paths: [@tmpdir]
      )
      assert_kind_of Array, results
      assert(results.any? { |p| p.include?('medium.txt') },
             'should find files matching size and time filters')
    end
  end

  describe 'time filtering with real file timestamps' do
    before do
      @tmpdir = Dir.mktmpdir('fdr_time_test')

      @old_file = File.join(@tmpdir, 'old_file.txt')
      @recent_file = File.join(@tmpdir, 'recent_file.txt')
      @very_recent_file = File.join(@tmpdir, 'very_recent_file.txt')

      File.write(@old_file, 'old content')
      File.write(@recent_file, 'recent content')
      File.write(@very_recent_file, 'very recent content')

      old_time = Time.now - (10 * 24 * 60 * 60)
      File.utime(old_time, old_time, @old_file)

      recent_time = Time.now - (5 * 24 * 60 * 60)
      File.utime(recent_time, recent_time, @recent_file)

      very_recent_time = Time.now - (1 * 24 * 60 * 60)
      File.utime(very_recent_time, very_recent_time, @very_recent_file)
    end

    after do
      FileUtils.rm_rf(@tmpdir) if @tmpdir && File.exist?(@tmpdir)
    end

    describe 'changed_within' do
      it 'finds files modified within the last 7 days' do
        results = Fdr.search(paths: [@tmpdir], changed_within: 604_800, type: 'f')

        assert results.any? { |path| File.basename(path) == 'recent_file.txt' }, 'should find recent_file.txt'
        assert results.any? { |path| File.basename(path) == 'very_recent_file.txt' }, 'should find very_recent_file.txt'
        refute results.any? { |path| File.basename(path) == 'old_file.txt' }, 'should not find old_file.txt'
      end

      it 'finds files modified within the last 2 days' do
        results = Fdr.search(paths: [@tmpdir], changed_within: 172_800, type: 'f')

        assert results.any? { |path| File.basename(path) == 'very_recent_file.txt' }, 'should find very_recent_file.txt'
        refute results.any? { |path| File.basename(path) == 'recent_file.txt' }, 'should not find recent_file.txt'
        refute results.any? { |path| File.basename(path) == 'old_file.txt' }, 'should not find old_file.txt'
      end

      it 'finds all files modified within the last 30 days' do
        results = Fdr.search(paths: [@tmpdir], changed_within: 2_592_000, type: 'f')

        assert results.any? { |path| File.basename(path) == 'old_file.txt' }, 'should find old_file.txt'
        assert results.any? { |path| File.basename(path) == 'recent_file.txt' }, 'should find recent_file.txt'
        assert results.any? { |path| File.basename(path) == 'very_recent_file.txt' }, 'should find very_recent_file.txt'
      end

      it 'finds no files with very short time window' do
        results = Fdr.search(paths: [@tmpdir], changed_within: 3600, type: 'f')

        refute(results.any? { |path| File.basename(path) == 'old_file.txt' })
        refute(results.any? { |path| File.basename(path) == 'recent_file.txt' })
        refute(results.any? { |path| File.basename(path) == 'very_recent_file.txt' })
      end
    end

    describe 'changed_before' do
      it 'finds files modified more than 7 days ago' do
        results = Fdr.search(paths: [@tmpdir], changed_before: 604_800, type: 'f')

        assert results.any? { |path| File.basename(path) == 'old_file.txt' }, 'should find old_file.txt'
        refute results.any? { |path| File.basename(path) == 'recent_file.txt' }, 'should not find recent_file.txt'
        refute results.any? { |path|
          File.basename(path) == 'very_recent_file.txt'
        }, 'should not find very_recent_file.txt'
      end

      it 'finds files modified more than 2 days ago' do
        results = Fdr.search(paths: [@tmpdir], changed_before: 172_800, type: 'f')

        assert results.any? { |path| File.basename(path) == 'old_file.txt' }, 'should find old_file.txt'
        assert results.any? { |path| File.basename(path) == 'recent_file.txt' }, 'should find recent_file.txt'
        refute results.any? { |path|
          File.basename(path) == 'very_recent_file.txt'
        }, 'should not find very_recent_file.txt'
      end

      it 'finds all files when using changed_before: 0' do
        results = Fdr.search(paths: [@tmpdir], changed_before: 0, type: 'f')

        assert(results.any? { |path| File.basename(path) == 'old_file.txt' })
        assert(results.any? { |path| File.basename(path) == 'recent_file.txt' })
        assert(results.any? { |path| File.basename(path) == 'very_recent_file.txt' })
      end
    end

    describe 'combining time filters' do
      it 'finds files in a specific time range' do
        results = Fdr.search(
          paths: [@tmpdir],
          changed_before: 172_800,
          changed_within: 604_800,
          type: 'f'
        )

        refute results.any? { |path| File.basename(path) == 'old_file.txt' }, 'should not find old_file.txt (too old)'
        assert results.any? { |path| File.basename(path) == 'recent_file.txt' }, 'should find recent_file.txt'
        refute results.any? { |path|
          File.basename(path) == 'very_recent_file.txt'
        }, 'should not find very_recent_file.txt (too recent)'
      end
    end

    describe 'time filters with pattern matching' do
      it 'combines time filtering with pattern matching' do
        results = Fdr.search(
          paths: [@tmpdir],
          pattern: /recent/,
          changed_within: 604_800,
          type: 'f'
        )

        assert(results.any? { |path| File.basename(path) == 'recent_file.txt' })
        assert(results.any? { |path| File.basename(path) == 'very_recent_file.txt' })
        refute(results.any? { |path| File.basename(path) == 'old_file.txt' })
      end

      it 'combines time filtering with extension' do
        rb_file = File.join(@tmpdir, 'script.rb')
        File.write(rb_file, 'ruby code')
        rb_time = Time.now - (1 * 24 * 60 * 60)
        File.utime(rb_time, rb_time, rb_file)

        results = Fdr.search(
          paths: [@tmpdir],
          extension: 'rb',
          changed_within: 172_800,
          type: 'f'
        )

        assert(results.any? { |path| File.basename(path) == 'script.rb' })
        refute(results.any? { |path| path.end_with?('.txt') })
      end
    end
  end
end
