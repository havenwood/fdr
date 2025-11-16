# frozen_string_literal: true

require_relative 'spec_helper'
require 'tmpdir'
require 'fileutils'

describe 'Fdr pattern matching' do
  describe 'exact filename matching' do
    it 'finds files by exact name' do
      results = Fdr.search(pattern: 'Cargo.toml', paths: ['ext'])

      assert(results.any? { |result| result.include?('Cargo.toml') },
             'should find Cargo.toml file')
      assert(results.all? { |result| result.include?('Cargo.toml') },
             'all results should match exact filename')
    end

    it 'finds version file by name' do
      results = Fdr.search(pattern: 'version', paths: ['lib'], max_depth: 2)

      assert(results.any? { |result| result.include?('version') },
             'should find version file')
    end
  end

  describe 'regex patterns' do
    it 'supports regex pattern matching with proper regex syntax' do
      results = Fdr.search(pattern: '.*_spec\.rb', paths: ['spec'], max_depth: 1)

      assert(results.any? { |result| result.match?(/.*_spec\.rb$/) },
             'should match regex pattern')
      assert(results.all? { |result| result.include?('_spec.rb') },
             'all results should match the regex pattern')
    end

    it 'matches patterns in filenames only by default (not full path)' do
      with_full = Fdr.search(pattern: 'ext', paths: ['.'], max_depth: 2, full_path: true)
      without_full = Fdr.search(pattern: 'ext', paths: ['.'], max_depth: 2, full_path: false)

      assert(with_full.any? { |p| p.include?('ext') },
             'full path should find ext in paths')
      assert(without_full.all? { |p| File.basename(p).include?('ext') || without_full.empty? },
             'filename only should match ext in basenames')
    end

    it 'supports character class patterns' do
      results = Fdr.search(pattern: '[a-z]+\.toml', paths: ['ext'], max_depth: 2)

      assert(results.any? { |result| result.include?('.toml') },
             'should match character class pattern')
      assert(results.all? { |result| result.end_with?('.toml') },
             'all results should end with .toml')
    end
  end

  describe 'case sensitivity' do
    before do
      @tmpdir = Dir.mktmpdir('fdr_case_test')
      @upper_file = File.join(@tmpdir, 'TestFile.txt')
      @lower_file = File.join(@tmpdir, 'testfile.txt')

      File.write(@upper_file, 'upper')
      File.write(@lower_file, 'lower')
    end

    after do
      FileUtils.rm_rf(@tmpdir) if @tmpdir && File.exist?(@tmpdir)
    end

    it 'is case insensitive by default' do
      lower = Fdr.search(pattern: 'testfile', paths: [@tmpdir], type: 'f')
      upper = Fdr.search(pattern: 'TestFile', paths: [@tmpdir], type: 'f')

      assert_equal lower.size, upper.size,
                   'case insensitive search should find same number of results'
      assert_operator lower.size, '>', 0, 'should find at least one match'
    end

    it 'respects case sensitivity when requested' do
      insensitive = Fdr.search(pattern: 'testfile', paths: [@tmpdir], type: 'f',
                               case_sensitive: false)
      sensitive_lower = Fdr.search(pattern: 'testfile', paths: [@tmpdir], type: 'f',
                                   case_sensitive: true)
      sensitive_upper = Fdr.search(pattern: 'TestFile', paths: [@tmpdir], type: 'f',
                                   case_sensitive: true)

      assert_operator insensitive.size, '>=', sensitive_lower.size,
                      'case insensitive should find at least as many as case sensitive'
      assert(sensitive_upper.any? { |p| p.include?('TestFile') },
             'case sensitive search for TestFile should find TestFile')
    end
  end

  describe 'full path matching' do
    it 'searches in full path when enabled' do
      with_full = Fdr.search(pattern: 'ext', paths: ['.'], full_path: true, max_depth: 2)
      without_full = Fdr.search(pattern: 'ext', paths: ['.'], full_path: false, max_depth: 2)

      assert(with_full.any? { |result| result.include?('ext') },
             'should match pattern in full path')
      assert_operator with_full.size, '>=', without_full.size,
                      'full path matching should find at least as many results'
    end

    it 'matches directory names in path with full_path option' do
      results = Fdr.search(pattern: 'fdr_native', paths: ['ext'], full_path: true, max_depth: 3)

      assert(results.any? { |result| result.include?('fdr_native') },
             'should match directory names in full path')
    end

    it 'finds more results with full_path than filename-only matching' do
      # Searching for 'lib' in full path should match files in lib directory
      with_full = Fdr.search(pattern: 'lib', paths: ['.'], full_path: true, max_depth: 2)
      without_full = Fdr.search(pattern: 'lib', paths: ['.'], full_path: false, max_depth: 2)

      # Full path should find files whose full path contains 'lib'
      assert_operator with_full.size, '>=', without_full.size,
                      'full path matching should find at least as many results'
    end
  end
end
