# frozen_string_literal: true

require "tmpdir"
require_relative "spec_helper"
require "fileutils"
require "timeout"

describe "Fdr error handling" do
  describe "invalid paths" do
    it "handles nonexistent paths gracefully" do
      results = search_results(paths: ["/nonexistent/path/xyz123"])
      assert_empty results, "nonexistent paths should return empty results"
    end

    it "returns no results for an empty paths array" do
      results = search_results(paths: [], max_depth: 1)
      assert_empty results
    end

    it "returns no results for an empty paths array with full_path" do
      results = search_results(pattern: "needle", paths: [], full_path: true)
      assert_empty results
    end

    it "defaults omitted paths to the current directory" do
      results = search_results(max_depth: 1)
      refute_empty results, "omitted paths should search the current directory"
    end

    it "rejects nil paths" do
      assert_raises(TypeError) { search_results(paths: nil) }
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
        assert_includes search_results(paths: [dir]), File.join(dir, "ok.txt")
        assert_equal [File.join(dir, "ok.txt")], grep_results(pattern: "needle", paths: [dir]).keys
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
        [{}, {column: true}, {byte_range: true}].each do |position|
          error = assert_raises(Fdr::IOError) do
            Fdr.grep(pattern: "needle", paths: [fifo], ignore_error: false, **position).to_a
          end
          assert_match(/fifo: not a regular file/, error.message)
        end
      end
    end

    it "raises when grep cannot open a file and ignore_error is false" do
      Dir.mktmpdir("fdr-unreadable-file") do |dir|
        path = File.join(dir, "secret.txt")
        File.write(path, "needle\n")
        File.chmod(0, path)

        assert_empty Fdr.grep(pattern: "needle", paths: [path]).to_a
        [{}, {column: true}, {byte_range: true}].each do |position|
          error = assert_raises(Fdr::IOError) do
            Fdr.grep(pattern: "needle", paths: [path], ignore_error: false, **position).to_a
          end
          assert_match(/secret\.txt/, error.message)
        end
      ensure
        File.chmod(0o600, path) if path && File.exist?(path)
      end
    end
  end

  describe "invalid patterns" do
    it "handles empty pattern by matching all files" do
      empty_pattern = search_results(pattern: "", paths: ["lib"], max_depth: 1)
      all_files = search_results(paths: ["lib"], max_depth: 1)
      assert_equal empty_pattern.size, all_files.size,
        "empty pattern should match all files"
    end

    it "handles nil pattern by matching all files" do
      nil_pattern = search_results(pattern: nil, paths: ["lib"], max_depth: 1)
      all_files = search_results(paths: ["lib"], max_depth: 1)
      refute_empty nil_pattern
      assert_equal nil_pattern.size, all_files.size,
        "nil pattern should match all files"
    end

    it "raises error for invalid regex pattern" do
      error = assert_raises(RegexpError) do
        search_results(pattern: "[invalid(regex", paths: ["."], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it "raises error for unclosed bracket in regex" do
      error = assert_raises(RegexpError) do
        search_results(pattern: "[abc", paths: ["."], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it "raises error for invalid named group in regex" do
      error = assert_raises(RegexpError) do
        search_results(pattern: "(?P<invalid)", paths: ["."], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it "raises error for invalid glob pattern" do
      error = assert_raises(ArgumentError) do
        search_results(pattern: "[invalid", paths: ["."], glob: true, max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end

    it "raises error for invalid exclude pattern" do
      error = assert_raises(ArgumentError) do
        search_results(paths: ["."], exclude: ["[invalid"], max_depth: 1)
      end
      assert_match(/Search failed/, error.message)
    end
  end

  describe "invalid depth values" do
    it "handles zero max_depth by returning empty results" do
      results = search_results(paths: ["."], max_depth: 0)
      assert_empty results, "max_depth: 0 should return no results"
    end

    it "raises error for negative max_depth" do
      error = assert_raises(ArgumentError) do
        search_results(paths: ["."], max_depth: -1)
      end
      assert_match(/max_depth must be a non-negative integer/, error.message)
    end

    it "raises error for negative min_depth" do
      error = assert_raises(ArgumentError) do
        search_results(paths: ["."], min_depth: -1)
      end
      assert_match(/min_depth must be a non-negative integer/, error.message)
    end

    it "handles min_depth greater than max_depth" do
      results = search_results(paths: ["."], min_depth: 5, max_depth: 2)
      assert_empty results
    end

    it "handles min_depth greater than max_depth in grep" do
      results = grep_results(pattern: "needle", paths: ["."], min_depth: 5, max_depth: 2)
      assert_empty results
    end
  end

  describe "invalid argument types" do
    it "raises TypeError when paths is not an Array" do
      assert_raises(TypeError) do
        search_results(paths: "lib")
      end
    end

    it "raises TypeError when extension is not a String" do
      assert_raises(TypeError) do
        search_results(paths: ["lib"], extension: :rb)
      end
    end

    it "raises TypeError when max_depth is not an Integer" do
      assert_raises(TypeError) do
        search_results(paths: ["lib"], max_depth: "1")
      end
    end

    it "raises TypeError when max_depth is a Float" do
      assert_raises(TypeError) do
        search_results(paths: ["lib"], max_depth: 2.9)
      end
    end

    it "raises TypeError when min_size is a Float" do
      assert_raises(TypeError) do
        search_results(paths: ["lib"], min_size: 1.5)
      end
    end

    it "raises TypeError when changed_within is a Float" do
      assert_raises(TypeError) do
        search_results(paths: ["lib"], changed_within: 0.5)
      end
    end

    it "raises TypeError when exclude is not an Array" do
      assert_raises(TypeError) do
        search_results(paths: ["lib"], exclude: "vendor")
      end
    end

    it "raises RangeError when min_size exceeds the native integer range" do
      assert_raises(RangeError) do
        search_results(paths: ["lib"], min_size: 2**70)
      end
    end

    it "raises TypeError when grep pattern is not a String" do
      assert_raises(TypeError) do
        grep_results(pattern: 42, paths: ["lib"])
      end
    end

    it "raises Fdr::InvalidType when grep pattern is nil" do
      assert_raises(Fdr::InvalidType) do
        grep_results(pattern: nil, paths: ["lib"])
      end
    end

    it "accepts truthy values for boolean kwargs" do
      Dir.mktmpdir("fdr-truthy") do |dir|
        File.write(File.join(dir, ".hidden.txt"), "")
        with_string = search_results(paths: [dir], hidden: "yes")
        with_true = search_results(paths: [dir], hidden: true)
        refute_empty with_string, "truthy hidden should include hidden files"
        assert_equal with_true, with_string, "truthy non-boolean should behave like true"
      end
    end
  end

  describe "invalid size and time values" do
    it "raises error for negative min_size" do
      error = assert_raises(ArgumentError) { search_results(paths: ["."], min_size: -1) }
      assert_match(/min_size must be a non-negative integer/, error.message)
    end

    it "raises error for negative max_size" do
      error = assert_raises(ArgumentError) { search_results(paths: ["."], max_size: -1) }
      assert_match(/max_size must be a non-negative integer/, error.message)
    end

    it "raises error for negative changed_within" do
      error = assert_raises(ArgumentError) { search_results(paths: ["."], changed_within: -1) }
      assert_match(/changed_within must be a non-negative integer/, error.message)
    end

    it "raises error for negative changed_before" do
      error = assert_raises(ArgumentError) { search_results(paths: ["."], changed_before: -1) }
      assert_match(/changed_before must be a non-negative integer/, error.message)
    end
  end

  describe "error classes" do
    it "tags the errors it raises itself while keeping their stdlib class" do
      [
        [Fdr::InvalidPattern, RegexpError, -> { search_results(pattern: "[", paths: ["lib"]) }],
        [Fdr::InvalidOption, ArgumentError, -> { search_results(type: "nope", paths: ["lib"]) }],
        [Fdr::InvalidOption, ArgumentError, -> { search_results(type: %w[f nope], paths: ["lib"]) }],
        [Fdr::InvalidType, TypeError, -> { search_results(paths: nil) }],
        [Fdr::InvalidType, TypeError, -> { search_results(exclude: [42], paths: ["lib"]) }],
        [Fdr::InvalidType, TypeError, -> { search_results(type: 42, paths: ["lib"]) }],
        [Fdr::InvalidType, TypeError, -> { search_results(type: [:f, 42], paths: ["lib"]) }],
        [Fdr::InvalidType, TypeError, -> { search_results(extension: [42], paths: ["lib"]) }],
        [Fdr::OutOfRange, RangeError, -> { search_results(max_depth: 2**64, paths: ["lib"]) }]
      ].each do |fdr_class, stdlib_class, call|
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
          Timeout.timeout(0.02) { grep_results(pattern: "(?i:(?:ha|hay|hays|haystac)+z)", paths: [path]) }
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
          search_results(pattern: "x", full_path: true, paths: ["."])
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
      refute_kind_of Fdr::Error, assert_raises(ArgumentError) { search_results(nope: true) }
      refute_kind_of Fdr::Error, assert_raises(ArgumentError) { grep_results(paths: []) }
    end

    it "leaves an exception raised by the caller's own coercion intact" do
      boom = Class.new(ArgumentError)
      raiser = Class.new { define_method(:to_str) { raise boom, "caller's problem" } }

      error = assert_raises(boom) { search_results(pattern: raiser.new, paths: ["lib"]) }

      refute_kind_of Fdr::Error, error
      assert_equal "caller's problem", error.message
    end

    it "accepts private implicit conversion methods" do
      stringish = Class.new do
        define_method(:to_str) { "fdr" }
        private :to_str
      end
      arrayish = Class.new do
        define_method(:to_ary) { ["lib"] }
        private :to_ary
      end
      pathish = Class.new do
        define_method(:to_path) { "lib" }
        private :to_path
      end
      extensions = Class.new do
        define_method(:to_ary) { ["rb"] }
        private :to_ary
      end

      assert_equal search_results(pattern: "fdr", paths: ["lib"]),
        search_results(pattern: stringish.new, paths: ["lib"])
      assert_equal search_results(paths: ["lib"]), search_results(paths: arrayish.new)
      assert_equal search_results(paths: ["lib"]), search_results(paths: [pathish.new])
      assert_equal search_results(paths: ["lib"], extension: "rb"),
        search_results(paths: ["lib"], extension: extensions.new)
    end

    it "leaves an exception raised by respond_to_missing? intact" do
      boom = Class.new(ArgumentError)
      raiser = Class.new do
        define_method(:respond_to_missing?) do |name, include_private|
          raise boom, "caller's problem" if name == :to_str

          super(name, include_private)
        end
      end

      error = assert_raises(boom) { search_results(pattern: raiser.new, paths: ["lib"]) }

      refute_kind_of Fdr::Error, error
      assert_equal "caller's problem", error.message
    end
  end

  describe "invalid options" do
    it "raises for a blank exclude glob, even with no paths" do
      assert_raises(ArgumentError) { search_results(paths: ["lib"], exclude: [""]) }
      assert_raises(ArgumentError) { search_results(paths: ["lib"], exclude: ["  "]) }
      assert_raises(ArgumentError) { search_results(paths: [], exclude: [""]) }
    end

    it "raises error for unknown file types" do
      error = assert_raises(ArgumentError) do
        search_results(type: "invalid", paths: ["."], max_depth: 1)
      end
      assert_match(/type must be one of/, error.message)
    end

    it "raises error for unknown symbolic file types" do
      error = assert_raises(ArgumentError) do
        search_results(type: :invalid, paths: ["."], max_depth: 1)
      end
      assert_match(/type must be one of/, error.message)
    end

    it "strips leading dots from extension" do
      with_dot = search_results(extension: ".rb", paths: ["lib"], max_depth: 1)
      without_dot = search_results(extension: "rb", paths: ["lib"], max_depth: 1)
      refute_empty with_dot, "a dotted extension should match like the bare extension"
      assert_equal without_dot, with_dot
      assert_equal without_dot, search_results(extension: "..rb", paths: ["lib"], max_depth: 1)
    end

    it "treats an empty extension as a trailing dot, as fd does" do
      Dir.mktmpdir("fdr-empty-extension") do |dir|
        trailing_dot = File.join(dir, "trailing.")
        File.write(trailing_dot, "")
        File.write(File.join(dir, "ordinary"), "")

        assert_equal [trailing_dot], search_results(extension: "", paths: [dir])
        assert_equal [trailing_dot], search_results(extension: ".", paths: [dir])
      end
    end

    it "handles nil extension by ignoring the filter" do
      with_nil = search_results(extension: nil, paths: ["lib"], max_depth: 1)
      without_ext = search_results(paths: ["lib"], max_depth: 1)
      assert_equal with_nil.size, without_ext.size,
        "nil extension should ignore extension filter"
    end
  end

  describe "edge cases" do
    it "yields nothing when no matches are found" do
      results = search_results(
        pattern: "nonexistent_pattern_xyz_123_abc",
        paths: ["."]
      )
      assert_empty results, "non-matching pattern should return empty"
    end

    it "handles very deep max_depth values by finding files" do
      results = search_results(paths: ["lib"], max_depth: 1000)
      refute_empty results, "very deep max_depth should find files"
      assert(results.all? { |p| p.start_with?("lib") },
        "all results should be from lib path")
    end

    it "handles special characters in patterns" do
      results = search_results(pattern: 'spec_helper\.rb$', paths: ["spec"], max_depth: 1)
      assert_equal ["spec/spec_helper.rb"], results
    end

    it "handles Unicode patterns" do
      Dir.mktmpdir do |dir|
        File.write(File.join(dir, "サンプル.txt"), "")

        results = search_results(pattern: "サンプル", paths: [dir])
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
          results = search_results(paths: [dir])
        ensure
          File.chmod(0o755, locked)
        end

        assert_includes results, File.join(dir, "readable.txt")
        refute_includes results, File.join(locked, "unreachable.txt")
      end
    end
  end
end
