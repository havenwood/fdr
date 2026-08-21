# frozen_string_literal: true

require "tmpdir"
require_relative "spec_helper"

describe Fdr do
  describe "module methods" do
    it "responds to .search" do
      assert_respond_to Fdr, :search
    end

    it "responds to .grep" do
      assert_respond_to Fdr, :grep
    end

    it "does not expose ambiguous search aliases" do
      refute_respond_to Fdr, :entries
      refute_respond_to Fdr, :scan
    end

    it "keeps .native_search private" do
      refute_respond_to Fdr, :native_search
      assert_raises(NoMethodError) { Fdr.native_search }
    end

    it "keeps .native_grep private" do
      refute_respond_to Fdr, :native_grep
      assert_raises(NoMethodError) { Fdr.native_grep }
    end
  end

  describe ".search" do
    it "returns an Array of results" do
      results = Fdr.search(paths: ["lib"], max_depth: 1)
      assert_kind_of Array, results
      refute_empty results, "should find files in lib directory"
    end

    it "returns path-sorted results" do
      Dir.mktmpdir("fdr-sort") do |dir|
        20.times do |i|
          subdir = File.join(dir, "dir#{i}")
          Dir.mkdir(subdir)
          10.times { |j| File.write(File.join(subdir, "file#{j}.txt"), "") }
        end

        results = Fdr.search(paths: [dir])
        assert_equal results.sort, results, "search results should be path-sorted"
      end
    end

    it "returns path-sorted results from the parallel walker" do
      Dir.mktmpdir("fdr-sort-parallel") do |dir|
        70.times do |index|
          subdir = File.join(dir, format("dir_%<index>03d", index:))
          Dir.mkdir(subdir)
          File.write(File.join(subdir, "file.txt"), "")
        end

        results = Fdr.search(paths: [dir])

        assert_equal 140, results.size
        assert_equal results.sort, results, "parallel search results should be path-sorted"
      end
    end

    it "tags result paths with the filesystem encoding" do
      results = Fdr.search(paths: ["lib"], max_depth: 1)

      refute_empty results
      assert(results.all? { |path| path.encoding == Encoding.find("filesystem") },
        "paths should carry the filesystem encoding")
    end

    it "returns String paths that point to existing files" do
      results = Fdr.search(paths: ["lib"], max_depth: 1)
      refute_empty results
      assert(results.all?(String),
        "all results should be String paths")
      assert(results.all? { |result| File.exist?(result) || File.symlink?(result) },
        "all paths should point to existing files or symlinks")
    end

    it "returns relative paths by default" do
      results = Fdr.search(paths: ["lib"], max_depth: 1)
      refute_empty results
      assert(results.all? { |result| !result.start_with?("/") },
        "paths should be relative, not absolute")
      assert(results.all? { |result| result.start_with?("lib") },
        "results should start with the search path")
    end

    it "accepts pattern as keyword argument and uses it" do
      with_pattern = Fdr.search(pattern: "fdr", paths: ["lib"], max_depth: 1)
      without_pattern = Fdr.search(paths: ["lib"], max_depth: 1)

      assert_operator with_pattern.size, "<=", without_pattern.size,
        "pattern should filter results"
      assert(with_pattern.any? { |p| p.include?("fdr") },
        "results should match the pattern")
    end

    it "accepts multiple paths and searches all of them" do
      results = Fdr.search(pattern: "spec", paths: %w[lib spec], max_depth: 2)

      refute_empty results, "should find spec matches"
      assert(results.any? { |p| p.start_with?("spec") },
        "results should include paths from spec directory")
    end

    it "accepts options with pattern and paths" do
      results = Fdr.search(pattern: "spec", paths: ["."], type: "d", max_depth: 2)

      assert_includes results, "./spec"
      assert(results.all? { |p| File.directory?(p) },
        "should only include directories with type: d")
    end

    it "returns empty array when pattern matches nothing" do
      results = Fdr.search(pattern: "nonexistent_xyz_123_abc", paths: ["."])

      assert_kind_of Array, results
      assert_empty results, "should return empty array for non-matching pattern"
    end
  end
end
