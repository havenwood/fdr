# frozen_string_literal: true

require "tmpdir"
require_relative "spec_helper"
require "fileutils"
require "timeout"

describe "Fdr error handling" do
  describe "invalid paths" do
    it "handles nonexistent paths gracefully" do
      results = Fdr.search(paths: ["/nonexistent/path/xyz123"])
      assert_kind_of Array, results
      assert_empty results, "nonexistent paths should return empty results"
    end

    it "returns no results for an empty paths array" do
      results = Fdr.search(paths: [], max_depth: 1)

      assert_kind_of Array, results
      assert_empty results
    end

    it "defaults omitted paths to the current directory" do
      results = Fdr.search(max_depth: 1)

      assert_kind_of Array, results
      refute_empty results, "omitted paths should search the current directory"
    end

    it "rejects nil paths" do
      assert_raises(TypeError) { Fdr.search(paths: nil) }
    end
  end

  describe "unreadable entries" do
    def with_unreadable
      Dir.mktmpdir("fdr-unreadable") do |dir|
        secret = File.join(dir, "secret")
        Dir.mkdir(secret)
        File.write(File.join(secret, "hidden.txt"), "needle\n")
        File.write(File.join(dir, "ok.txt"), "needle\n")
        File.chmod(0, secret)
        begin
          yield dir
        ensure
          File.chmod(0o755, secret)
        end
      end
    end

    it "skips what it cannot read by default, as fd does" do
      with_unreadable do |dir|
        assert_includes Fdr.search(paths: [dir]), File.join(dir, "ok.txt")
        assert_equal [File.join(dir, "ok.txt")], Fdr.grep(pattern: "needle", paths: [dir]).keys
      end
    end

    it "raises for an unreadable entry when ignore_error is false" do
      with_unreadable do |dir|
        error = assert_raises(Fdr::IOError) do
          Fdr.search(paths: [dir], ignore_error: false).to_a
        end

        assert_match(/secret/, error.message)
        assert_raises(Fdr::IOError) do
          Fdr.grep(pattern: "needle", paths: [dir], ignore_error: false).to_a
        end
      end
    end

    it "treats nil ignore_error as falsey" do
      with_unreadable do |dir|
        assert_raises(Fdr::IOError) do
          Fdr.search(paths: [dir], ignore_error: nil).to_a
        end
        assert_raises(Fdr::IOError) do
          Fdr.grep(pattern: "needle", paths: [dir], ignore_error: nil).to_a
        end
      end
    end

    it "does not raise on a readable tree when ignore_error is false" do
      refute_empty Fdr.search(paths: ["lib"], ignore_error: false).to_a
    end
    it "raises for an explicit non-file when ignore_error is false" do
      Dir.mktmpdir("fdr-non-file") do |dir|
        fifo = File.join(dir, "fifo")
        File.mkfifo(fifo)

        assert_empty Fdr.grep(pattern: "needle", paths: [fifo]).to_a
        assert_empty Fdr.grep(pattern: "needle", paths: [dir], ignore_error: false).to_a
        error = assert_raises(Fdr::IOError) do
          Fdr.grep(pattern: "needle", paths: [fifo], ignore_error: false).to_a
        end
        assert_match(/fifo: not a regular file/, error.message)
      end
    end
  end

  describe "invalid patterns" do
    it "handles empty pattern by matching all files" do
      empty_pattern = Fdr.search(pattern: "", paths: ["lib"], max_depth: 1)
      all_files = Fdr.search(paths: ["lib"], max_depth: 1)
      assert_kind_of Array, empty_pattern
      assert_equal empty_pattern.size, all_files.size,
        "empty pattern should match all files"
    end

    it "handles nil pattern by matching all files" do
      nil_pattern = Fdr.search(pattern: nil, paths: ["lib"], max_depth: 1)
      all_files = Fdr.search(paths: ["lib"], max_depth: 1)
      assert_kind_of Array, nil_pattern
      refute_empty nil_pattern
      assert_equal nil_pattern.size, all_files.size,
        "nil pattern should match all files"
    end

    it "raises error for invalid regex pattern" do
      error = assert_raises(RegexpError) do
        Fdr.search(pattern: "[invalid(regex", paths: ["."], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it "raises error for unclosed bracket in regex" do
      error = assert_raises(RegexpError) do
        Fdr.search(pattern: "[abc", paths: ["."], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it "raises error for invalid named group in regex" do
      error = assert_raises(RegexpError) do
        Fdr.search(pattern: "(?P<invalid)", paths: ["."], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it "raises error for invalid glob pattern" do
      error = assert_raises(ArgumentError) do
        Fdr.search(pattern: "[invalid", paths: ["."], glob: true, max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it "raises error for invalid exclude pattern" do
      error = assert_raises(ArgumentError) do
        Fdr.search(paths: ["."], exclude: ["[invalid"], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end
  end

  describe "invalid depth values" do
    it "handles zero max_depth by returning empty results" do
      results = Fdr.search(paths: ["."], max_depth: 0)
      assert_kind_of Array, results
      assert_empty results, "max_depth: 0 should return no results"
    end

    it "raises error for negative max_depth" do
      error = assert_raises(ArgumentError) do
        Fdr.search(paths: ["."], max_depth: -1)
      end
      assert_match(/max_depth must be a non-negative integer/, error.message)
    end

    it "raises error for negative min_depth" do
      error = assert_raises(ArgumentError) do
        Fdr.search(paths: ["."], min_depth: -1)
      end
      assert_match(/min_depth must be a non-negative integer/, error.message)
    end

    it "handles min_depth greater than max_depth" do
      results = Fdr.search(paths: ["."], min_depth: 5, max_depth: 2)
      assert_kind_of Array, results
      assert_empty results
    end

    it "handles min_depth greater than max_depth in grep" do
      results = Fdr.grep(pattern: "needle", paths: ["."], min_depth: 5, max_depth: 2)
      assert_kind_of Hash, results
      assert_empty results
    end
  end

  describe "invalid argument types" do
    it "raises TypeError when paths is not an Array" do
      assert_raises(TypeError) do
        Fdr.search(paths: "lib")
      end
    end

    it "raises TypeError when extension is not a String" do
      assert_raises(TypeError) do
        Fdr.search(paths: ["lib"], extension: :rb)
      end
    end

    it "raises TypeError when max_depth is not an Integer" do
      assert_raises(TypeError) do
        Fdr.search(paths: ["lib"], max_depth: "1")
      end
    end

    it "raises TypeError when max_depth is a Float" do
      assert_raises(TypeError) do
        Fdr.search(paths: ["lib"], max_depth: 2.9)
      end
    end

    it "raises TypeError when min_size is a Float" do
      assert_raises(TypeError) do
        Fdr.search(paths: ["lib"], min_size: 1.5)
      end
    end

    it "raises TypeError when changed_within is a Float" do
      assert_raises(TypeError) do
        Fdr.search(paths: ["lib"], changed_within: 0.5)
      end
    end

    it "raises TypeError when exclude is not an Array" do
      assert_raises(TypeError) do
        Fdr.search(paths: ["lib"], exclude: "vendor")
      end
    end

    it "raises RangeError when min_size exceeds the native integer range" do
      assert_raises(RangeError) do
        Fdr.search(paths: ["lib"], min_size: 2**70)
      end
    end

    it "raises TypeError when grep pattern is not a String" do
      assert_raises(TypeError) do
        Fdr.grep(pattern: 42, paths: ["lib"])
      end
    end

    it "raises Fdr::InvalidType when grep pattern is nil" do
      assert_raises(Fdr::InvalidType) do
        Fdr.grep(pattern: nil, paths: ["lib"])
      end
    end

    it "accepts truthy values for boolean kwargs" do
      Dir.mktmpdir("fdr-truthy") do |dir|
        File.write(File.join(dir, ".hidden.txt"), "")
        with_string = Fdr.search(paths: [dir], hidden: "yes")
        with_true = Fdr.search(paths: [dir], hidden: true)
        refute_empty with_string, "truthy hidden should include hidden files"
        assert_equal with_true, with_string, "truthy non-boolean should behave like true"
      end
    end
  end

  describe "invalid size and time values" do
    it "raises error for negative min_size" do
      error = assert_raises(ArgumentError) { Fdr.search(paths: ["."], min_size: -1) }
      assert_match(/min_size must be a non-negative integer/, error.message)
    end

    it "raises error for negative max_size" do
      error = assert_raises(ArgumentError) { Fdr.search(paths: ["."], max_size: -1) }
      assert_match(/max_size must be a non-negative integer/, error.message)
    end

    it "raises error for negative changed_within" do
      error = assert_raises(ArgumentError) { Fdr.search(paths: ["."], changed_within: -1) }
      assert_match(/changed_within must be a non-negative integer/, error.message)
    end

    it "raises error for negative changed_before" do
      error = assert_raises(ArgumentError) { Fdr.search(paths: ["."], changed_before: -1) }
      assert_match(/changed_before must be a non-negative integer/, error.message)
    end
  end

  describe "error classes" do
    it "tags the errors it raises itself while keeping their stdlib class" do
      {
        Fdr::InvalidPattern => [RegexpError, -> { Fdr.search(pattern: "[", paths: ["lib"]) }],
        Fdr::InvalidOption => [ArgumentError, -> { Fdr.search(type: "nope", paths: ["lib"]) }],
        Fdr::InvalidType => [TypeError, -> { Fdr.search(paths: nil) }],
        Fdr::OutOfRange => [RangeError, -> { Fdr.search(max_depth: 2**64, paths: ["lib"]) }]
      }.each do |fdr_class, (stdlib_class, call)|
        error = assert_raises(fdr_class) { call.call }

        assert_kind_of Fdr::Error, error
        assert_kind_of stdlib_class, error
      end
    end

    it "leaves an interrupt raised during a search untagged" do
      Dir.mktmpdir("fdr-interrupt") do |dir|
        path = File.join(dir, "big.txt")
        File.open(path, "wb") { |file| 40.times { file.write("haystack\n" * (1024 * 1024 / 9)) } }

        error = assert_raises(Timeout::Error) do
          Timeout.timeout(0.02) { Fdr.grep(pattern: "(?i:(?:ha|hay|hays|haystac)+z)", paths: [path]) }
        end

        refute_kind_of Fdr::Error, error
      end
    end

    it "raises Fdr::IOError when the working directory is gone" do
      skip "fork is unavailable" unless Process.respond_to?(:fork)

      reader, writer = IO.pipe
      pid = fork do
        reader.close
        dir = Dir.mktmpdir("fdr-gone")
        Dir.chdir(dir)
        FileUtils.remove_entry(dir)
        begin
          Fdr.search(pattern: "x", full_path: true, paths: ["."])
          writer.puts "no raise"
        rescue => e
          writer.puts "#{e.class} #{e.is_a?(Fdr::Error)} #{e.is_a?(IOError)}"
        end
        writer.close
        exit! 0
      end
      writer.close
      result = reader.read.strip
      reader.close
      Process.waitpid(pid)

      assert_equal "Fdr::IOError true true", result
    end

    it "does not tag Ruby's own keyword errors" do
      refute_kind_of Fdr::Error, assert_raises(ArgumentError) { Fdr.search(nope: true) }
      refute_kind_of Fdr::Error, assert_raises(ArgumentError) { Fdr.grep(paths: []) }
    end

    it "leaves an exception raised by the caller's own coercion intact" do
      boom = Class.new(ArgumentError)
      raiser = Class.new { define_method(:to_str) { raise boom, "caller's problem" } }

      error = assert_raises(boom) { Fdr.search(pattern: raiser.new, paths: ["lib"]) }

      refute_kind_of Fdr::Error, error
      assert_equal "caller's problem", error.message
    end
  end

  describe "invalid options" do
    it "raises for a blank exclude glob, even with no paths" do
      assert_raises(ArgumentError) { Fdr.search(paths: ["lib"], exclude: [""]) }
      assert_raises(ArgumentError) { Fdr.search(paths: ["lib"], exclude: ["  "]) }
      assert_raises(ArgumentError) { Fdr.search(paths: [], exclude: [""]) }
    end

    it "raises error for unknown file types" do
      error = assert_raises(ArgumentError) do
        Fdr.search(type: "invalid", paths: ["."], max_depth: 1)
      end
      assert_match(/type must be one of/, error.message)
    end

    it "raises error for unknown symbolic file types" do
      error = assert_raises(ArgumentError) do
        Fdr.search(type: :invalid, paths: ["."], max_depth: 1)
      end
      assert_match(/type must be one of/, error.message)
    end

    it "validates array members" do
      [
        [Fdr::InvalidOption, -> { Fdr.search(type: %w[f invalid], paths: ["lib"]) }],
        [Fdr::InvalidType, -> { Fdr.search(type: [:f, 42], paths: ["lib"]) }],
        [Fdr::InvalidType, -> { Fdr.search(extension: [42], paths: ["lib"]) }]
      ].each do |error_class, call|
        assert_raises(error_class) { call.call }
      end
    end

    it "strips leading dots from extension" do
      with_dot = Fdr.search(extension: ".rb", paths: ["lib"], max_depth: 1)
      without_dot = Fdr.search(extension: "rb", paths: ["lib"], max_depth: 1)
      refute_empty with_dot, "a dotted extension should match like the bare extension"
      assert_equal without_dot, with_dot
      assert_equal without_dot, Fdr.search(extension: "..rb", paths: ["lib"], max_depth: 1)
    end

    it "treats an empty extension as a trailing dot, as fd does" do
      Dir.mktmpdir("fdr-empty-extension") do |dir|
        trailing_dot = File.join(dir, "trailing.")
        File.write(trailing_dot, "")
        File.write(File.join(dir, "ordinary"), "")

        assert_equal [trailing_dot], Fdr.search(extension: "", paths: [dir])
        assert_equal [trailing_dot], Fdr.search(extension: ".", paths: [dir])
      end
    end

    it "handles nil extension by ignoring the filter" do
      with_nil = Fdr.search(extension: nil, paths: ["lib"], max_depth: 1)
      without_ext = Fdr.search(paths: ["lib"], max_depth: 1)
      assert_kind_of Array, with_nil
      assert_equal with_nil.size, without_ext.size,
        "nil extension should ignore extension filter"
    end
  end

  describe "edge cases" do
    it "returns empty array when no matches found" do
      results = Fdr.search(
        pattern: "nonexistent_pattern_xyz_123_abc",
        paths: ["."]
      )
      assert_kind_of Array, results
      assert_empty results, "non-matching pattern should return empty"
    end

    it "handles very deep max_depth values by finding files" do
      results = Fdr.search(paths: ["lib"], max_depth: 1000)
      assert_kind_of Array, results
      refute_empty results, "very deep max_depth should find files"
      assert(results.all? { |p| p.start_with?("lib") },
        "all results should be from lib path")
    end

    it "handles special characters in patterns" do
      results = Fdr.search(pattern: 'spec_helper\.rb$', paths: ["spec"], max_depth: 1)
      assert_equal ["spec/spec_helper.rb"], results
    end

    it "handles Unicode patterns" do
      Dir.mktmpdir do |dir|
        File.write(File.join(dir, "サンプル.txt"), "")

        results = Fdr.search(pattern: "サンプル", paths: [dir])
        assert_equal 1, results.size
      end
    end
  end

  describe "permission errors" do
    it "continues searching when encountering permission errors" do
      skip "chmod 0 does not restrict root or Windows" if Gem.win_platform? || Process.uid.zero?

      Dir.mktmpdir do |dir|
        File.write(File.join(dir, "readable.txt"), "")
        locked = File.join(dir, "locked")
        Dir.mkdir(locked)
        File.write(File.join(locked, "unreachable.txt"), "")
        File.chmod(0o000, locked)

        begin
          results = Fdr.search(paths: [dir])
        ensure
          File.chmod(0o755, locked)
        end

        assert_includes results, File.join(dir, "readable.txt")
        refute_includes results, File.join(locked, "unreachable.txt")
      end
    end
  end
end
