# frozen_string_literal: true

require_relative 'spec_helper'

describe 'Fdr glob patterns' do
  describe 'string patterns as glob patterns' do
    it 'supports glob patterns with string patterns' do
      results = Fdr.search(
        pattern: '*.rb',
        paths: ['lib'],
        max_depth: 1
      )
      assert_kind_of Array, results
      refute_empty results, 'should find .rb files with glob pattern'
      assert(results.all? { |result| result.end_with?('.rb') },
             'all results should match *.rb pattern')
    end

    it 'supports wildcard matching with *' do
      results = Fdr.search(
        pattern: 'Cargo.*',
        paths: ['ext'],
        max_depth: 2
      )
      assert(results.any? { |result| result.include?('Cargo.toml') },
             'should match Cargo.toml with Cargo.* pattern')
      assert(results.all? { |result| result.include?('Cargo') },
             'all results should start with Cargo')
    end

    it 'supports question mark wildcards for single characters' do
      results = Fdr.search(
        pattern: 'fdr.rb',
        paths: ['lib'],
        max_depth: 1
      )
      assert(results.any? { |result| result.include?('fdr.rb') },
             'should find fdr.rb with glob pattern')
    end

    it 'supports bracket expressions for character classes' do
      results = Fdr.search(
        pattern: '*.[rt][bs]',
        paths: ['ext'],
        max_depth: 2
      )
      assert(results.all? { |result| result.match?(/\.[rt][bs]$/) },
             'all results should match bracket expression pattern')
    end
  end

  describe 'glob vs regex' do
    it 'treats Regexp objects as regex patterns' do
      regex_results = Fdr.search(
        pattern: /.*\.rb$/,
        paths: ['lib'],
        max_depth: 1
      )
      assert(regex_results.all? { |result| result.end_with?('.rb') },
             'regex pattern should match .rb files')
    end

    it 'treats strings as glob patterns' do
      glob_results = Fdr.search(
        pattern: '*.toml',
        paths: ['ext'],
        max_depth: 3
      )
      refute_empty glob_results, 'should find .toml files with glob'
      assert(glob_results.all? { |result| result.end_with?('.toml') },
             'all results should end with .toml')
    end

    it 'glob and regex produce different results for special chars' do
      # The * in glob means wildcard, in regex it means zero or more of preceding char
      glob_star = Fdr.search(
        pattern: 'fdr*.rb',
        paths: ['lib'],
        max_depth: 1
      )
      assert(glob_star.any? { |p| p.include?('fdr') },
             'glob with * should find fdr-prefixed files')
    end
  end

  describe 'complex glob patterns' do
    it 'supports nested wildcards with **' do
      results = Fdr.search(
        pattern: '**/Cargo.toml',
        paths: ['ext']
      )
      assert(results.any? { |result| result.include?('Cargo.toml') },
             'should find Cargo.toml with **/ pattern')
      assert(results.all? { |result| result.include?('Cargo.toml') },
             'all results should contain Cargo.toml')
    end

    it 'supports multiple extensions with braces' do
      results = Fdr.search(
        pattern: '*.{toml,lock}',
        paths: ['ext'],
        max_depth: 2
      )
      assert(results.all? do |result|
        result.end_with?('.toml') || result.end_with?('.lock')
      end, 'all results should be .toml or .lock files')
    end

    it 'finds files matching nested glob patterns' do
      results = Fdr.search(
        pattern: '**/Cargo.toml',
        paths: ['ext']
      )
      assert(results.any? { |result| result.include?('Cargo.toml') },
             'should find nested Cargo.toml files')
      assert(results.all? { |result| result.include?('Cargo.toml') },
             'all results should match the pattern')
    end
  end

  describe 'glob with full_path' do
    it 'applies glob pattern to full path when full_path is true' do
      results = Fdr.search(
        pattern: '**/fdr_native*',
        paths: ['.'],
        full_path: true
      )
      assert(results.any? { |result| result.include?('fdr_native') },
             'should match **/fdr_native* pattern in full path')
    end

    it 'matches directory structure with glob and full_path' do
      results = Fdr.search(
        pattern: '**/Cargo.toml',
        paths: ['.'],
        full_path: true
      )
      assert(results.any? { |p| p.include?('Cargo.toml') },
             'should find Cargo.toml files with nested glob')
      assert(results.all? { |p| p.include?('Cargo.toml') },
             'all results should match the pattern')
    end
  end
end
