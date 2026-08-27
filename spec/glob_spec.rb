# frozen_string_literal: true

require_relative "spec_helper"
require "tmpdir"

describe "Seen glob patterns" do
  describe "glob option" do
    it "supports glob patterns when glob option is enabled" do
      results = path_results(
        pattern: "*.rb",
        paths: ["lib"],
        glob: true,
        max_depth: 1
      )
      refute_empty results, "should find .rb files with glob pattern"
      assert(results.all? { |result| result.end_with?(".rb") },
        "all results should match *.rb pattern")
    end

    it "matches glob patterns case-insensitively by default" do
      Dir.mktmpdir("seen-glob-case") do |dir|
        File.write(File.join(dir, "README.MD"), "")

        insensitive = path_results(pattern: "readme.*", paths: [dir], glob: true)
        sensitive = path_results(pattern: "readme.*", paths: [dir], glob: true, case_sensitive: true)

        refute_empty insensitive, "glob should ignore case by default"
        assert_empty sensitive, "case_sensitive should apply to glob patterns"
      end
    end

    it "supports wildcard matching with *" do
      results = path_results(
        pattern: "Cargo.*",
        paths: ["ext"],
        glob: true,
        max_depth: 2
      )
      assert(results.any? { |result| result.include?("Cargo.toml") },
        "should match Cargo.toml with Cargo.* pattern")
      assert(results.all? { |result| result.include?("Cargo") },
        "all results should start with Cargo")
    end

    it "supports question mark wildcards for single characters" do
      results = path_results(
        pattern: "s?en.rb",
        paths: ["lib"],
        glob: true,
        max_depth: 1
      )
      assert(results.any? { |result| result.include?("seen.rb") },
        "should find seen.rb with glob pattern")
    end

    it "supports bracket expressions for character classes" do
      results = path_results(
        pattern: "*.[rt][bs]",
        paths: ["ext"],
        glob: true,
        max_depth: 2
      )
      assert(results.all? { |result| result.match?(/\.[rt][bs]$/) },
        "all results should match bracket expression pattern")
    end
  end

  describe "glob vs regex" do
    it "treats pattern as regex by default (not glob)" do
      regex_results = path_results(
        pattern: '.*\.rb$',
        paths: ["lib"],
        glob: false,
        max_depth: 1
      )
      assert(regex_results.all? { |result| result.end_with?(".rb") },
        "regex pattern should match .rb files")
    end

    it "treats pattern as glob when glob option is true" do
      glob_results = path_results(
        pattern: "*.toml",
        paths: ["ext"],
        glob: true,
        max_depth: 3
      )
      refute_empty glob_results, "should find .toml files with glob"
      assert(glob_results.all? { |result| result.end_with?(".toml") },
        "all results should end with .toml")
    end

    it "glob and regex produce different results for special chars" do
      # Glob `*` is a wildcard, while regex `*` repeats the preceding token.
      glob_results = path_results(pattern: "en*", paths: ["lib"], glob: true, max_depth: 1)
      regex_results = path_results(pattern: "en*", paths: ["lib"], glob: false, max_depth: 1)

      assert_empty glob_results, "glob must match the whole name, so en* matches nothing in lib"
      refute_empty regex_results, "regex matches a substring, so en* matches seen files"
    end
  end

  describe "complex glob patterns" do
    it "supports nested wildcards with **" do
      results = path_results(
        pattern: "**/Cargo.toml",
        paths: ["ext"],
        glob: true,
        full_path: true
      )
      assert(results.any? { |result| result.include?("Cargo.toml") },
        "should find Cargo.toml with **/ pattern")
      assert(results.all? { |result| result.include?("Cargo.toml") },
        "all results should contain Cargo.toml")
    end

    it "supports multiple extensions with braces" do
      results = path_results(
        pattern: "*.{toml,lock}",
        paths: ["ext"],
        glob: true,
        max_depth: 2
      )
      assert(results.all? do |result|
        result.end_with?(".toml", ".lock")
      end, "all results should be .toml or .lock files")
    end
  end

  describe "glob with full_path" do
    it "applies glob pattern to full path when full_path is true" do
      results = path_results(
        pattern: "**/seen_native*",
        paths: ["."],
        glob: true,
        full_path: true
      )
      assert(results.any? { |result| result.include?("seen_native") },
        "should match **/seen_native* pattern in full path")
    end

    it "applies glob to full paths under an absolute search root" do
      Dir.mktmpdir("seen_glob_full_path_test") do |tmpdir|
        src = File.join(tmpdir, "src")
        Dir.mkdir(src)
        File.write(File.join(src, "main.rb"), "x")
        File.write(File.join(tmpdir, "other.rb"), "x")

        results = path_results(pattern: "**/src/*.rb", paths: [tmpdir], glob: true, full_path: true)

        assert_equal [File.join(src, "main.rb")], results
      end
    end

    it "matches directory structure with glob and full_path" do
      results = path_results(
        pattern: "**/Cargo.toml",
        paths: ["."],
        glob: true,
        full_path: true
      )
      assert(results.any? { |p| p.include?("Cargo.toml") },
        "should find Cargo.toml files with nested glob")
      assert(results.all? { |p| p.include?("Cargo.toml") },
        "all results should match the pattern")
    end
  end
end
