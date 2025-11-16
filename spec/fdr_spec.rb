# frozen_string_literal: true

require_relative 'spec_helper'

describe Fdr do
  describe 'module methods' do
    it 'responds to .search' do
      assert Fdr.respond_to?(:search), 'Fdr.search method should exist'
    end

    it 'responds to .entries alias' do
      assert Fdr.respond_to?(:entries), 'Fdr.entries method should exist'
    end

    it 'responds to .scan alias' do
      assert Fdr.respond_to?(:scan), 'Fdr.scan method should exist'
    end
  end

  describe '.search' do
    it 'returns an Array of results' do
      results = Fdr.search(paths: ['lib'], max_depth: 1)
      assert_kind_of Array, results
      refute_empty results, 'should find files in lib directory'
    end

    it 'returns String paths that point to existing files' do
      results = Fdr.search(paths: ['lib'], max_depth: 1)
      refute_empty results
      assert(results.all? { |result| result.is_a?(String) },
             'all results should be String paths')
      assert(results.all? { |result| File.exist?(result) || File.symlink?(result) },
             'all paths should point to existing files or symlinks')
    end

    it 'returns relative paths by default' do
      results = Fdr.search(paths: ['lib'], max_depth: 1)
      refute_empty results
      assert(results.all? { |result| !result.start_with?('/') },
             'paths should be relative, not absolute')
      assert(results.all? { |result| result.start_with?('lib') },
             'results should start with the search path')
    end

    it 'accepts pattern as keyword argument and uses it' do
      with_pattern = Fdr.search(pattern: 'fdr', paths: ['lib'], max_depth: 1)
      without_pattern = Fdr.search(paths: ['lib'], max_depth: 1)

      assert_operator with_pattern.size, '<=', without_pattern.size,
                      'pattern should filter results'
      assert(with_pattern.any? { |p| p.include?('fdr') },
             'results should match the pattern')
    end

    it 'accepts multiple paths and searches all of them' do
      results = Fdr.search(pattern: 'spec', paths: %w[lib spec], max_depth: 2)

      refute_empty results, 'should find spec matches'
      assert(results.any? { |p| p.start_with?('spec') },
             'results should include paths from spec directory')
    end

    it 'accepts options with pattern and paths' do
      results = Fdr.search(pattern: 'test', paths: ['.'], type: 'd', max_depth: 2)

      assert_kind_of Array, results
      # Verify type filter is applied
      assert(results.none? { |p| File.file?(p) && !File.directory?(p) },
             'should only include directories with type: d')
    end

    it 'returns empty array when pattern matches nothing' do
      results = Fdr.search(pattern: 'nonexistent_xyz_123_abc', paths: ['.'])

      assert_kind_of Array, results
      assert_empty results, 'should return empty array for non-matching pattern'
    end
  end
end
