# frozen_string_literal: true

require_relative 'spec_helper'

describe 'Fdr filtering' do
  describe 'extension filtering' do
    it 'filters by single extension' do
      results = Fdr.search(extension: 'toml', paths: ['ext'], max_depth: 3)
      refute_empty results
      assert(results.all? { |result| result.end_with?('.toml') })
    end

    it 'finds rb files' do
      results = Fdr.search(extension: 'rb', paths: ['lib'])
      assert(results.all? { |result| result.end_with?('.rb') })
      assert(results.any? { |result| result.include?('fdr.rb') })
    end

    it 'works with extension without leading dot' do
      results = Fdr.search(extension: 'rb', paths: ['lib'], max_depth: 2)
      refute_empty results
      assert(results.all? { |result| result.end_with?('.rb') })
    end
  end

  describe 'file type filtering' do
    it 'finds files only with type f' do
      results = Fdr.search(type: 'f', paths: ['lib'], max_depth: 2)
      refute_empty results
    end

    it 'finds directories only with type d' do
      results = Fdr.search(type: 'd', paths: ['.'], max_depth: 2)
      refute_empty results
      assert(results.all? { |path| File.directory?(path) })
    end

    it 'excludes directories when type is f' do
      results = Fdr.search(type: 'f', paths: ['.'], max_depth: 1)
      assert(results.none? { |path| File.directory?(path) })
    end

    it 'supports file type alias' do
      results = Fdr.search(type: 'file', paths: ['lib'], max_depth: 2)
      refute_empty results
      assert(results.all? { |path| File.file?(path) })
    end

    it 'supports dir type alias' do
      results = Fdr.search(type: 'dir', paths: ['.'], max_depth: 2)
      refute_empty results
      assert(results.all? { |path| File.directory?(path) })
    end

    it 'supports directory type alias' do
      results = Fdr.search(type: 'directory', paths: ['.'], max_depth: 2)
      refute_empty results
      assert(results.all? { |path| File.directory?(path) })
    end

    it 'supports symlink type alias' do
      results = Fdr.search(type: 'symlink', paths: ['.'], max_depth: 3, hidden: true)
      assert_kind_of Array, results
      results.each do |path|
        assert File.symlink?(path) if File.exist?(path)
      end
    end
  end

  describe 'hidden files' do
    it 'excludes hidden files by default' do
      results = Fdr.search(paths: ['.'], max_depth: 1)
      hidden_files = results.select do |result|
        basename = File.basename(result)
        basename.start_with?('.') && basename != '.'
      end
      assert_empty hidden_files
    end

    it 'includes hidden files when requested' do
      results = Fdr.search(paths: ['.'], max_depth: 1, hidden: true)
      assert(results.any? { |result| File.basename(result).start_with?('.') })
    end

    it 'finds dotfiles with hidden option' do
      results = Fdr.search(pattern: /gitignore/, paths: ['.'], hidden: true, max_depth: 1)
      assert(results.any? { |result| result.include?('.gitignore') })
    end
  end
end
