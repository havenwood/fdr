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
    it "returns an Enumerator" do
      results = Fdr.search(paths: ["lib"], max_depth: 1)

      assert_kind_of Enumerator, results
      assert_kind_of String, results.first
    end

    it "yields to a block and returns the Enumerator" do
      yielded = []

      results = Fdr.search(paths: ["lib"], max_depth: 1) { |path| yielded << path }

      assert_kind_of Enumerator, results
      assert_equal results.to_a.sort, yielded.sort
    end

    it "does not search when given no block" do
      Dir.mktmpdir("fdr-unconsumed") do |dir|
        Fdr.search(paths: [dir], type: "f")
        path = File.join(dir, "created_after_search.txt")
        File.write(path, "")

        assert_equal [path], Fdr.search(paths: [dir], type: "f").to_a
      end
    end

    it "starts searching when the Enumerator is consumed" do
      Dir.mktmpdir("fdr-lazy") do |dir|
        results = Fdr.search(paths: [dir], type: "f")
        path = File.join(dir, "created_after_search.txt")
        File.write(path, "")

        assert_includes results.to_a, path
      end
    end

    it "can be enumerated again" do
      results = Fdr.search(paths: ["lib"], max_depth: 1)
      first = results.to_a.sort

      refute_empty first
      assert_equal first, results.to_a.sort
    end

    it "restarts after rewind" do
      results = Fdr.search(paths: ["lib"], max_depth: 1)
      first = results.next
      results.rewind

      assert_includes results.to_a, first
    end

    it "omits the ./ prefix when paths defaults, as fd does" do
      results = Fdr.search(pattern: "^LICENSE$").to_a

      assert_equal ["LICENSE"], results
    end

    it "keeps the ./ prefix for an explicit . root, as fd does" do
      results = Fdr.search(pattern: "^LICENSE$", paths: ["."]).to_a

      assert_equal ["./LICENSE"], results
    end

    it "echoes back whichever root it was given" do
      assert_equal ["lib/fdr.rb"], Fdr.search(pattern: '^fdr\.rb$', paths: ["lib"]).to_a
      assert_equal ["./lib/fdr.rb"], Fdr.search(pattern: '^fdr\.rb$', paths: ["./lib"]).to_a
    end

    it "emits results again for a repeated path" do
      results = Fdr.search(pattern: '^fdr\.rb$', paths: %w[lib lib], type: "f").to_a

      assert_equal ["lib/fdr.rb", "lib/fdr.rb"], results.sort
    end

    it "tags result paths with the filesystem encoding" do
      results = search_results(paths: ["lib"], max_depth: 1)

      refute_empty results
      assert(results.all? { |path| path.encoding == Encoding.find("filesystem") },
        "paths should carry the filesystem encoding")
    end

    it "returns String paths that point to existing files" do
      results = search_results(paths: ["lib"], max_depth: 1)
      refute_empty results
      assert(results.all?(String),
        "all results should be String paths")
      assert(results.all? { |result| File.exist?(result) || File.symlink?(result) },
        "all paths should point to existing files or symlinks")
    end

    it "returns relative paths by default" do
      results = search_results(paths: ["lib"], max_depth: 1)
      refute_empty results
      assert(results.all? { |result| !result.start_with?("/") },
        "paths should be relative, not absolute")
      assert(results.all? { |result| result.start_with?("lib") },
        "results should start with the search path")
    end

    it "accepts pattern as keyword argument and uses it" do
      with_pattern = search_results(pattern: "fdr", paths: ["lib"], max_depth: 1)
      without_pattern = search_results(paths: ["lib"], max_depth: 1)

      assert_operator with_pattern.size, "<=", without_pattern.size,
        "pattern should filter results"
      assert(with_pattern.any? { |p| p.include?("fdr") },
        "results should match the pattern")
    end

    it "accepts multiple paths and searches all of them" do
      results = search_results(pattern: "spec", paths: %w[lib spec], max_depth: 2)

      refute_empty results, "should find spec matches"
      assert(results.any? { |p| p.start_with?("spec") },
        "results should include paths from spec directory")
    end

    it "accepts options with pattern and paths" do
      results = search_results(pattern: "spec", paths: ["."], type: "d", max_depth: 2)

      assert_includes results, "./spec"
      assert(results.all? { |p| File.directory?(p) },
        "should only include directories with type: d")
    end

    it "yields nothing when pattern matches nothing" do
      results = search_results(pattern: "nonexistent_xyz_123_abc", paths: ["."])
      assert_empty results
    end
  end
end
