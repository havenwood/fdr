# frozen_string_literal: true

require_relative "spec_helper"

describe "Fdr.search" do
  describe "basic searches" do
    it "finds files with pattern" do
      results = search_results(pattern: "version", paths: ["lib"], max_depth: 2)

      refute_empty results, "should find files matching pattern"
      assert(results.any? { |p| p.include?("version") },
        "results should match the pattern")
    end

    it "supports type filter" do
      results = search_results(pattern: "lib", paths: ["."], type: "d", max_depth: 2)

      refute_empty results, "should find directories"
      assert(results.all? { |p| File.directory?(p) },
        "all results should be directories with type: d")
    end

    it "finds with single path" do
      results = search_results(pattern: "fdr", paths: ["lib"])

      refute_empty results, "should find matches in single path"
      assert(results.all? { |p| p.start_with?("lib") },
        "all results should be from the specified path")
    end
  end

  describe "edge cases" do
    it "returns all files when no pattern is given" do
      without_pattern = search_results(paths: ["lib"], max_depth: 1)

      refute_empty without_pattern, "should return files when no pattern given"
      assert(without_pattern.all?(String),
        "results should be strings")
    end

    it "handles nonexistent paths gracefully" do
      results = search_results(paths: ["/nonexistent/path/12345"])

      assert_empty results, "should yield nothing for a nonexistent path"
    end

    it "accepts empty exclude array" do
      results_no_exclude = search_results(extension: "rb", paths: ["lib"])
      results_empty_exclude = search_results(extension: "rb", paths: ["lib"], exclude: [])

      assert_equal results_no_exclude.size, results_empty_exclude.size,
        "empty exclude array should not affect results"
    end

    it "yields nothing for an impossible filter combination" do
      results = search_results(
        pattern: "nonexistent_file_xyz_123",
        extension: "xyz",
        paths: ["."]
      )
      assert_empty results, "impossible filter combination should return empty"
    end
  end

  describe "combining filters" do
    it "combines pattern and extension filters" do
      results = search_results(
        pattern: "lib",
        extension: "rs",
        paths: ["ext"],
        type: "f",
        max_depth: 4
      )

      assert(results.all? { |result| result.end_with?(".rs") },
        "all results should have .rs extension")
      assert(results.all? { |result| result.include?("lib") },
        "all results should match the pattern")
      assert(results.none? { |result| File.directory?(result) },
        "should only include files with type: f")
    end

    it "combines type and extension filters" do
      results = search_results(
        extension: "rb",
        type: "f",
        paths: ["lib"]
      )

      refute_empty results, "should find matching .rb files"
      assert(results.all? { |result| result.end_with?(".rb") },
        "all results should have .rb extension")
      assert(results.all? { |result| File.file?(result) },
        "all results should be files")
      assert(results.none? { |result| File.directory?(result) },
        "should not include directories")
    end

    it "combines pattern, extension and depth filters" do
      results = search_results(
        pattern: "spec",
        extension: "rb",
        paths: ["spec"],
        max_depth: 2,
        type: "f"
      )

      refute_empty results, "should find matching spec files"
      assert(results.all? { |p| p.end_with?(".rb") },
        "should only have .rb files")
      assert(results.all? { |p| p.include?("spec") },
        "should match pattern")
      assert(results.all? { |p| File.file?(p) },
        "all results should be files")
    end
  end
end
