# frozen_string_literal: true

require_relative 'spec_helper'
require 'fileutils'
require 'tmpdir'

describe 'Fdr.grep' do
  before do
    @tmpdir = Dir.mktmpdir('fdr_grep_test')
    @path = File.join(@tmpdir, 'example.rb')
    File.write(@path, "first\nNeedle\nneedle\nneedle twice needle\n")
  end

  after do
    FileUtils.rm_rf(@tmpdir) if @tmpdir && File.exist?(@tmpdir)
  end

  describe 'results' do
    it 'returns a Hash of paths to one-based matching line numbers' do
      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir])

      assert_equal({@path => [3, 4]}, results)
    end

    it 'reports a line matching more than once only once' do
      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir])

      assert_equal [3, 4], results[@path]
    end

    it 'returns an empty Hash when nothing matches' do
      results = Fdr.grep(pattern: 'haystack', paths: [@tmpdir])

      assert_empty results
    end

    it 'orders paths deterministically' do
      100.times do |index|
        dir = File.join(@tmpdir, format('dir_%<index>03d', index:))
        Dir.mkdir(dir)
        File.write(File.join(dir, 'file.txt'), "needle\n")
      end

      keys = Fdr.grep(pattern: 'needle', paths: [@tmpdir]).keys

      assert_equal keys.sort, keys, 'paths should be sorted'
    end
  end

  describe 'content matching' do
    it 'is case sensitive by default' do
      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir])

      assert_equal [3, 4], results[@path]
    end

    it 'can search case insensitively' do
      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir], case_sensitive: false)

      assert_equal [2, 3, 4], results[@path]
    end

    it 'skips binary files' do
      File.binwrite(File.join(@tmpdir, 'binary.bin'), "needle\n\0needle\n")

      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir])

      assert_equal({@path => [3, 4]}, results)
    end
  end

  describe 'file selection' do
    it 'skips hidden files by default' do
      File.write(File.join(@tmpdir, '.hidden.rb'), "needle\n")

      default_results = Fdr.grep(pattern: 'needle', paths: [@tmpdir])
      with_hidden = Fdr.grep(pattern: 'needle', paths: [@tmpdir], hidden: true)

      assert_equal [@path], default_results.keys
      assert_equal 2, with_hidden.size
    end

    it 'respects gitignore by default' do
      Dir.mkdir(File.join(@tmpdir, '.git'))
      File.write(File.join(@tmpdir, '.gitignore'), "ignored.rb\n")
      File.write(File.join(@tmpdir, 'ignored.rb'), "needle\n")

      default_results = Fdr.grep(pattern: 'needle', paths: [@tmpdir])
      without_ignore = Fdr.grep(pattern: 'needle', paths: [@tmpdir], no_ignore: true)

      assert_equal [@path], default_results.keys
      assert_equal 2, without_ignore.size
    end

    it 'skips excluded patterns' do
      Dir.mkdir(File.join(@tmpdir, 'vendor'))
      File.write(File.join(@tmpdir, 'vendor', 'skip.rb'), "needle\n")

      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir], exclude: %w[vendor])

      assert_equal [@path], results.keys
    end

    it 'searches every given path' do
      other = Dir.mktmpdir('fdr_grep_other')
      other_path = File.join(other, 'other.rb')
      File.write(other_path, "needle\n")

      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir, other])

      assert_equal [@path, other_path].sort, results.keys.sort
    ensure
      FileUtils.rm_rf(other)
    end

    it 'filters by extension' do
      File.write(File.join(@tmpdir, 'notes.txt'), "needle\n")

      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir], extension: 'rb')

      assert_equal [@path], results.keys
    end

    it 'filters by filename with name' do
      File.write(File.join(@tmpdir, 'example_spec.rb'), "needle\n")

      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir], name: '_spec\.rb$')

      assert_equal [File.join(@tmpdir, 'example_spec.rb')], results.keys
    end

    it 'respects max_depth' do
      nested = File.join(@tmpdir, 'nested')
      Dir.mkdir(nested)
      File.write(File.join(nested, 'deep.rb'), "needle\n")

      results = Fdr.grep(pattern: 'needle', paths: [@tmpdir], max_depth: 1)

      assert_equal [@path], results.keys
    end
  end

  describe 'errors' do
    it 'requires a pattern' do
      assert_raises(ArgumentError) { Fdr.grep(paths: [@tmpdir]) }
    end

    it 'raises for an invalid regex pattern' do
      error = assert_raises(ArgumentError) do
        Fdr.grep(pattern: '[invalid', paths: [@tmpdir])
      end

      assert_match(/Grep failed/, error.message)
    end

    it 'raises for a pattern spanning lines' do
      error = assert_raises(ArgumentError) do
        Fdr.grep(pattern: "first\nNeedle", paths: [@tmpdir])
      end

      assert_match(/Grep failed/, error.message)
    end
  end
end
