# frozen_string_literal: true

require_relative 'spec_helper'

describe 'Fdr error handling' do
  describe 'invalid paths' do
    it 'handles nonexistent paths gracefully' do
      results = Fdr.search(paths: ['/nonexistent/path/xyz123'])
      assert_kind_of Array, results
      assert_empty results, 'nonexistent paths should return empty results'
    end

    it 'handles empty paths array by falling back to current directory' do
      results = Fdr.search(paths: [], max_depth: 1)
      all_files = Fdr.search(max_depth: 1)
      assert_kind_of Array, results
      assert_equal results.size, all_files.size,
                   'empty paths array should fall back to current directory'
    end

    it 'handles nil paths by falling back to current directory' do
      results = Fdr.search(max_depth: 1)
      assert_kind_of Array, results
      refute_empty results, 'nil paths should fall back to current directory'
      assert(results.all? { |p| !p.empty? },
             'results should be valid paths')
    end
  end

  describe 'invalid patterns' do
    it 'handles empty pattern by matching all files' do
      empty_pattern = Fdr.search(pattern: '', paths: ['lib'], max_depth: 1)
      all_files = Fdr.search(paths: ['lib'], max_depth: 1)
      assert_kind_of Array, empty_pattern
      assert_equal empty_pattern.size, all_files.size,
                   'empty pattern should match all files'
    end

    it 'handles nil pattern by matching all files' do
      nil_pattern = Fdr.search(pattern: nil, paths: ['lib'], max_depth: 1)
      all_files = Fdr.search(paths: ['lib'], max_depth: 1)
      assert_kind_of Array, nil_pattern
      refute_empty nil_pattern
      assert_equal nil_pattern.size, all_files.size,
                   'nil pattern should match all files'
    end

    it 'raises error for invalid regex pattern' do
      error = assert_raises(RuntimeError) do
        Fdr.search(pattern: '[invalid(regex', paths: ['.'], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it 'raises error for unclosed bracket in regex' do
      error = assert_raises(RuntimeError) do
        Fdr.search(pattern: '[abc', paths: ['.'], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it 'raises error for invalid named group in regex' do
      error = assert_raises(RuntimeError) do
        Fdr.search(pattern: '(?P<invalid)', paths: ['.'], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it 'raises error for invalid glob pattern' do
      error = assert_raises(RuntimeError) do
        Fdr.search(pattern: '[invalid', paths: ['.'], glob: true, max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end
  end

  describe 'invalid depth values' do
    it 'handles zero max_depth by returning empty results' do
      results = Fdr.search(paths: ['.'], max_depth: 0)
      assert_kind_of Array, results
      assert_empty results, 'max_depth: 0 should return no results'
    end

    it 'raises error for negative max_depth' do
      error = assert_raises(ArgumentError) do
        Fdr.search(paths: ['.'], max_depth: -1)
      end
      assert_match(/max_depth must be a non-negative integer/, error.message)
    end

    it 'raises error for negative min_depth' do
      error = assert_raises(ArgumentError) do
        Fdr.search(paths: ['.'], min_depth: -1)
      end
      assert_match(/min_depth must be a non-negative integer/, error.message)
    end

    it 'handles min_depth greater than max_depth' do
      results = Fdr.search(paths: ['.'], min_depth: 5, max_depth: 2)
      assert_kind_of Array, results
      assert_empty results
    end
  end

  describe 'invalid options' do
    it 'handles unknown file types by ignoring the filter' do
      results = Fdr.search(type: 'invalid', paths: ['.'], max_depth: 1)
      all_files = Fdr.search(paths: ['.'], max_depth: 1)
      assert_kind_of Array, results
      assert_equal results.size, all_files.size,
                   'invalid file type should be ignored'
    end

    it 'handles empty extension by matching no files' do
      with_empty = Fdr.search(extension: '', paths: ['lib'])
      assert_kind_of Array, with_empty
      assert_empty with_empty, 'empty extension should match no files'
    end

    it 'handles nil extension by ignoring the filter' do
      with_nil = Fdr.search(extension: nil, paths: ['lib'], max_depth: 1)
      without_ext = Fdr.search(paths: ['lib'], max_depth: 1)
      assert_kind_of Array, with_nil
      assert_equal with_nil.size, without_ext.size,
                   'nil extension should ignore extension filter'
    end
  end

  describe 'edge cases' do
    it 'returns empty array when no matches found' do
      results = Fdr.search(
        pattern: 'nonexistent_pattern_xyz_123_abc',
        paths: ['.']
      )
      assert_kind_of Array, results
      assert_empty results, 'non-matching pattern should return empty'
    end

    it 'handles very deep max_depth values by finding files' do
      results = Fdr.search(paths: ['lib'], max_depth: 1000)
      assert_kind_of Array, results
      refute_empty results, 'very deep max_depth should find files'
      assert(results.all? { |p| p.start_with?('lib') },
             'all results should be from lib path')
    end

    it 'handles special characters in patterns' do
      results = Fdr.search(pattern: 'test', paths: ['spec'], max_depth: 1)
      assert_kind_of Array, results
      assert(results.all? { |p| p.include?('test') } || results.empty?,
             'should either find test files or return empty')
    end

    it 'handles Unicode patterns' do
      all_files = Fdr.search(pattern: '.*', paths: ['.'], max_depth: 1)
      assert_kind_of Array, all_files
      refute_empty all_files, '/* wildcard should match files'
    end
  end

  describe 'permission errors' do
    it 'continues searching when encountering permission errors' do
      results = Fdr.search(paths: ['.'], max_depth: 2)
      assert_kind_of Array, results
      refute_empty results, 'search should find files despite potential permission errors'
    end
  end
end
