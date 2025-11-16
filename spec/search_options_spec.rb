# frozen_string_literal: true

require_relative 'spec_helper'
require 'tmpdir'
require 'fileutils'

describe 'Fdr search options' do
  describe 'depth control' do
    it 'respects max_depth option' do
      shallow = Fdr.search(paths: ['.'], max_depth: 1)
      deep = Fdr.search(paths: ['.'], max_depth: 3)

      assert deep.size > shallow.size,
             'deeper search should find more files'
    end

    it 'respects min_depth option' do
      results = Fdr.search(paths: ['ext'], min_depth: 2, max_depth: 3)

      refute_empty results, 'should find files at min_depth 2'
    end

    it 'combines min_depth and max_depth correctly' do
      depth_2_only = Fdr.search(paths: ['ext'], min_depth: 2, max_depth: 2)
      less_restricted = Fdr.search(paths: ['ext'], min_depth: 2, max_depth: 3)

      assert_operator depth_2_only.size, '<=', less_restricted.size,
                      'broader depth range should find at least as many files'
    end

    it 'returns empty when min_depth exceeds directory depth' do
      results = Fdr.search(paths: ['lib'], min_depth: 100, max_depth: 100)

      assert_kind_of Array, results
      assert_empty results, 'should return empty when min_depth exceeds actual depth'
    end

    it 'max_depth with 1 finds top level items' do
      results = Fdr.search(paths: ['lib'], max_depth: 1)

      refute_empty results
      assert(results.all? { |p| !p.include?('lib/fdr/') },
             'max depth 1 should not include nested subdirectories')
    end
  end

  describe 'path handling' do
    it 'searches multiple paths' do
      results = Fdr.search(extension: 'rb', paths: %w[lib spec], max_depth: 2)

      assert(results.any? { |result| result.start_with?('lib') },
             'should find files in lib directory')
      assert(results.any? { |result| result.start_with?('spec') },
             'should find files in spec directory')
    end

    it 'defaults to current directory when paths not specified' do
      results = Fdr.search(extension: 'md', max_depth: 1)

      assert(results.any? { |result| result.include?('README') },
             'should find README.md in current directory')
    end

    it 'handles single path as array' do
      results = Fdr.search(paths: ['lib'], max_depth: 1)

      refute_empty results, 'should find files in lib'
      assert(results.all? { |result| result.start_with?('lib') },
             'all results should be from lib directory')
    end

    it 'returns relative paths' do
      results = Fdr.search(paths: ['lib'], max_depth: 1)

      refute_empty results
      assert(results.none? { |result| result.start_with?('/') },
             'paths should be relative, not absolute')
    end

    it 'limits results to specified paths only' do
      lib_only = Fdr.search(extension: 'rb', paths: ['lib'], max_depth: 1)
      spec_only = Fdr.search(extension: 'rb', paths: ['spec'], max_depth: 1)

      assert(lib_only.all? { |p| p.start_with?('lib') },
             'lib search should only return lib files')
      assert(spec_only.all? { |p| p.start_with?('spec') },
             'spec search should only return spec files')
    end
  end

  describe 'exclude patterns' do
    it 'filters out excluded paths' do
      all_results = Fdr.search(extension: 'toml', paths: ['ext'])
      filtered_results = Fdr.search(
        extension: 'toml',
        paths: ['ext'],
        exclude: ['ffi']
      )

      assert filtered_results.size < all_results.size,
             'excluding ffi should reduce results'
      refute(filtered_results.any? { |result| result.include?('/ffi/') || result.start_with?('ext/ffi') },
             'results should not contain ffi directory')
    end

    it 'accepts multiple exclusion patterns' do
      results = Fdr.search(
        extension: 'rs',
        paths: ['ext'],
        exclude: %w[ffi build]
      )

      assert_kind_of Array, results
      refute(results.any? { |p| p.include?('/ffi/') },
             'should exclude ffi directory')
    end

    it 'works with empty exclude array' do
      results_no_exclude = Fdr.search(paths: ['lib'])
      results_empty_exclude = Fdr.search(paths: ['lib'], exclude: [])

      assert_equal results_no_exclude.size, results_empty_exclude.size,
                   'empty exclude array should not affect results'
    end

    it 'actually excludes paths from results' do
      all_ext = Fdr.search(paths: ['ext'], type: 'f', extension: 'toml')
      without_core = Fdr.search(paths: ['ext'], type: 'f', extension: 'toml', exclude: ['core'])

      assert_operator without_core.size, '<=', all_ext.size,
                      'excluding core should find fewer or equal files'
      refute(without_core.any? { |p| p.include?('/core/') || p.include?('core/') },
             'excluded core directory should not appear in results')
    end
  end

  describe 'no_ignore option' do
    before do
      @tmpdir = Dir.mktmpdir('fdr_ignore_test')
      Dir.mkdir(File.join(@tmpdir, '.git'))

      @normal_file = File.join(@tmpdir, 'normal.txt')
      @ignored_file = File.join(@tmpdir, 'ignored.txt')
      @subdir = File.join(@tmpdir, 'subdir')
      @subdir_file = File.join(@subdir, 'nested.txt')

      File.write(@normal_file, 'normal')
      File.write(@ignored_file, 'ignored')
      Dir.mkdir(@subdir)
      File.write(@subdir_file, 'nested')

      gitignore = File.join(@tmpdir, '.gitignore')
      File.write(gitignore, "ignored.txt\n")
    end

    after do
      FileUtils.rm_rf(@tmpdir) if @tmpdir && File.exist?(@tmpdir)
    end

    it 'respects .gitignore by default' do
      with_ignore = Fdr.search(paths: [@tmpdir], type: 'f')

      assert(with_ignore.any? { |result| result.include?('normal.txt') },
             'should find non-ignored file')
      refute(with_ignore.any? { |result| result.include?('ignored.txt') },
             'should NOT find ignored file')
    end

    it 'ignores .gitignore when no_ignore is true' do
      without_ignore = Fdr.search(paths: [@tmpdir], type: 'f', no_ignore: true)

      assert(without_ignore.any? { |result| result.include?('ignored.txt') },
             'should find ignored file when no_ignore: true')
      assert(without_ignore.any? { |result| result.include?('normal.txt') },
             'should still find normal file')
    end

    it 'finds more files with no_ignore than with default ignore' do
      with_ignore = Fdr.search(paths: [@tmpdir], type: 'f')
      without_ignore = Fdr.search(paths: [@tmpdir], type: 'f', no_ignore: true)

      assert without_ignore.size > with_ignore.size,
             'no_ignore should find more files than respecting .gitignore'
    end
  end
end
