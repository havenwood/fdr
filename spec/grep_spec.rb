# frozen_string_literal: true

require_relative "spec_helper"
require "fileutils"
require "tmpdir"

describe "Fdr.grep" do
  before do
    @tmpdir = Dir.mktmpdir("fdr_grep_test")
    @path = File.join(@tmpdir, "example.rb")
    File.write(@path, "first\nNeedle\nneedle\nneedle twice needle\n")
  end

  after do
    FileUtils.rm_rf(@tmpdir) if @tmpdir && File.exist?(@tmpdir)
  end

  describe "results" do
    it "returns an Enumerator of path, one-based line number and text" do
      results = Fdr.grep(pattern: "needle", paths: [@tmpdir])

      assert_kind_of Enumerator, results
      assert_equal [[@path, 3, "needle"], [@path, 4, "needle twice needle"]], results.to_a.sort
    end

    it "yields to a block and returns the Enumerator" do
      yielded = []

      results = Fdr.grep(pattern: "needle", paths: [@tmpdir]) { |*match| yielded << match }

      assert_kind_of Enumerator, results
      assert_equal results.to_a.sort, yielded.sort
    end

    it "groups matches by path" do
      results = Fdr.grep(pattern: "needle", paths: [@tmpdir]).group_by { |path,| path }

      assert_equal [[@path, 3, "needle"], [@path, 4, "needle twice needle"]], results[@path].sort
    end

    it "leaves a location-keyed Hash to the caller, which dedupes" do
      results = Fdr.grep(pattern: "needle", paths: [@path, @path])
        .to_h { |path, line_number, text| [[path, line_number], text] }

      assert_equal({[@path, 3] => "needle", [@path, 4] => "needle twice needle"}, results)
    end

    it "reports a line with two matches once" do
      results = Fdr.grep(pattern: "needle", paths: [@tmpdir]).to_a

      assert_equal [[@path, 4, "needle twice needle"]], results.filter { |_path, line_number, _text| line_number == 4 }
    end

    it "omits the ./ prefix when paths defaults, as rg does" do
      path, = Fdr.grep(pattern: "module Fdr", name: '^version\.rb$').first

      assert_equal "lib/fdr/version.rb", path
    end

    it "reports one match per occurrence with column, as rg --vimgrep does" do
      results = Fdr.grep(pattern: "needle", paths: [@tmpdir], column: true).to_a.sort

      assert_equal [[@path, 3, 1, "needle"],
        [@path, 4, 1, "needle twice needle"],
        [@path, 4, 14, "needle twice needle"]], results
    end

    it "counts the column in bytes, as rg does" do
      path = File.join(@tmpdir, "multibyte.txt")
      File.write(path, "caf\u00e9 needle here\n")

      _, _, column, text = Fdr.grep(pattern: "needle", paths: [path], column: true).first

      assert_equal 7, column
      assert_equal "needle", text.byteslice(column - 1, 6)
    end

    it "yields nothing when nothing matches" do
      results = Fdr.grep(pattern: "haystack", paths: [@tmpdir])

      assert_empty results.to_a
    end

    it "starts grepping when the Enumerator is consumed" do
      results = Fdr.grep(pattern: "later", paths: [@tmpdir])
      path = File.join(@tmpdir, "later.txt")
      File.write(path, "later\n")

      assert_includes results.to_a, [path, 1, "later"]
    end

    it "can be enumerated again" do
      results = Fdr.grep(pattern: "needle", paths: [@tmpdir])
      first = results.to_a.sort

      assert_equal first, results.to_a.sort
    end

    it "emits matches again for a repeated path" do
      results = Fdr.grep(pattern: "needle", paths: [@path, @path]).to_a

      assert_equal 2, results.count { |match| match == [@path, 3, "needle"] }
      assert_equal 2, results.count { |match| match == [@path, 4, "needle twice needle"] }
    end

    it "tags matching lines with the external encoding" do
      _, _, line = Fdr.grep(pattern: "needle", paths: [@tmpdir]).find do |_path, line_number, _text|
        line_number == 3
      end

      assert_equal Encoding.default_external, line.encoding
    end
  end

  describe "content matching" do
    it "is case sensitive by default" do
      results = grep_results(pattern: "needle", paths: [@tmpdir])

      assert_equal [3, 4], results[@path].keys
    end

    it "can search case insensitively" do
      results = grep_results(pattern: "needle", paths: [@tmpdir], content_case_sensitive: false)

      assert_equal [2, 3, 4], results[@path].keys
    end

    it "treats nil case sensitivity as falsey" do
      results = grep_results(pattern: "needle", paths: [@tmpdir], content_case_sensitive: nil)

      assert_equal [2, 3, 4], results[@path].keys
    end

    it "scans binary files when text is true, as rg -a does" do
      binary = File.join(@tmpdir, "binary.bin")
      File.binwrite(binary, "before\0needle here\n")

      assert_empty grep_results(pattern: "needle here", paths: [binary])
      assert_equal({binary => {1 => "before\0needle here"}},
        grep_results(pattern: "needle here", paths: [binary], text: true))
    end

    it "allows a NUL in the pattern only when text is true" do
      binary = File.join(@tmpdir, "binary.bin")
      File.binwrite(binary, "before\0needle here\n")

      [{}, {column: true}].each do |shape|
        assert_empty Fdr.grep(pattern: "e\0n", paths: [binary], **shape).to_a
        refute_empty Fdr.grep(pattern: "e\0n", paths: [binary], text: true, **shape).to_a
      end
    end

    it "skips binary files" do
      File.binwrite(File.join(@tmpdir, "binary.bin"), "needle\n\0needle\n")

      results = grep_results(pattern: "needle", paths: [@tmpdir])

      assert_equal [@path], results.keys
    end
  end

  describe "file selection" do
    it "skips hidden files by default" do
      File.write(File.join(@tmpdir, ".hidden.rb"), "needle\n")

      default_results = grep_results(pattern: "needle", paths: [@tmpdir])
      with_hidden = grep_results(pattern: "needle", paths: [@tmpdir], hidden: true)

      assert_equal [@path], default_results.keys
      assert_equal 2, with_hidden.size
    end

    it "respects gitignore by default" do
      Dir.mkdir(File.join(@tmpdir, ".git"))
      File.write(File.join(@tmpdir, ".gitignore"), "ignored.rb\n")
      File.write(File.join(@tmpdir, "ignored.rb"), "needle\n")

      default_results = grep_results(pattern: "needle", paths: [@tmpdir])
      without_ignore = grep_results(pattern: "needle", paths: [@tmpdir], no_ignore: true)

      assert_equal [@path], default_results.keys
      assert_equal 2, without_ignore.size
    end

    it "skips excluded patterns" do
      Dir.mkdir(File.join(@tmpdir, "vendor"))
      File.write(File.join(@tmpdir, "vendor", "skip.rb"), "needle\n")

      results = grep_results(pattern: "needle", paths: [@tmpdir], exclude: %w[vendor])

      assert_equal [@path], results.keys
    end

    it "searches every given path" do
      other = Dir.mktmpdir("fdr_grep_other")
      other_path = File.join(other, "other.rb")
      File.write(other_path, "needle\n")

      results = grep_results(pattern: "needle", paths: [@tmpdir, other])

      assert_equal [@path, other_path].sort, results.keys.sort
    ensure
      FileUtils.rm_rf(other)
    end

    it "returns no results for an empty paths array" do
      assert_empty grep_results(pattern: "needle", paths: [])
    end

    it "rejects nil paths" do
      assert_raises(TypeError) { grep_results(pattern: "needle", paths: nil) }
    end

    it "filters by size" do
      big = File.join(@tmpdir, "big.txt")
      File.write(big, "needle\n#{"padding\n" * 100}")

      results = grep_results(pattern: "needle", paths: [@tmpdir], max_size: 100)

      assert_includes results.keys, @path
      refute_includes results.keys, big
    end

    it "filters by modification time" do
      old = File.join(@tmpdir, "old.txt")
      File.write(old, "needle\n")
      File.utime(Time.now - 86_400, Time.now - 86_400, old)

      results = grep_results(pattern: "needle", paths: [@tmpdir], changed_within: 3600)

      assert_includes results.keys, @path
      refute_includes results.keys, old
    end

    it "filters by extension" do
      File.write(File.join(@tmpdir, "notes.txt"), "needle\n")

      results = grep_results(pattern: "needle", paths: [@tmpdir], extension: "rb")

      assert_equal [@path], results.keys
    end

    it "filters by multiple extensions" do
      notes = File.join(@tmpdir, "notes.txt")
      File.write(notes, "needle\n")

      results = grep_results(pattern: "needle", paths: [@tmpdir], extension: %w[rb txt])

      assert_equal [@path, notes].sort, results.keys.sort
    end

    it "filters by filename with name" do
      File.write(File.join(@tmpdir, "example_spec.rb"), "needle\n")

      results = grep_results(pattern: "needle", paths: [@tmpdir], name: '_spec\.rb$')

      assert_equal [File.join(@tmpdir, "example_spec.rb")], results.keys
    end

    it "filters names case insensitively by default" do
      upper_path = File.join(@tmpdir, "UPPER.rb")
      File.write(upper_path, "needle\n")

      default_results = grep_results(pattern: "needle", paths: [@tmpdir], name: "upper")
      sensitive = grep_results(pattern: "needle", paths: [@tmpdir], name: "upper", case_sensitive: true)

      assert_equal [upper_path], default_results.keys
      assert_empty sensitive
    end

    it "filters names with a glob when glob is true" do
      File.write(File.join(@tmpdir, "notes.txt"), "needle\n")

      results = grep_results(pattern: "needle", paths: [@tmpdir], name: "*.rb", glob: true)

      assert_equal [@path], results.keys
    end

    it "respects max_depth" do
      nested = File.join(@tmpdir, "nested")
      Dir.mkdir(nested)
      File.write(File.join(nested, "deep.rb"), "needle\n")

      results = grep_results(pattern: "needle", paths: [@tmpdir], max_depth: 1)

      assert_equal [@path], results.keys
    end

    it "keeps directory pruning active with min_depth" do
      Dir.mkdir(File.join(@tmpdir, ".git"))
      File.write(File.join(@tmpdir, ".gitignore"), "ignored/\n")
      %w[.hidden ignored excluded visible].each do |directory|
        Dir.mkdir(File.join(@tmpdir, directory))
        File.write(File.join(@tmpdir, directory, "match.rb"), "needle\n")
      end

      results = grep_results(
        pattern: "needle",
        paths: [@tmpdir],
        min_depth: 2,
        exclude: %w[excluded]
      )

      assert_equal [File.join(@tmpdir, "visible", "match.rb")], results.keys
    end
  end

  describe "errors" do
    it "requires a pattern" do
      assert_raises(ArgumentError) { grep_results(paths: [@tmpdir]) }
    end

    it "rejects the search-only type filter" do
      assert_raises(ArgumentError) do
        grep_results(pattern: "needle", paths: [@tmpdir], type: "f")
      end
    end

    it "raises for an invalid regex pattern" do
      error = assert_raises(RegexpError) do
        grep_results(pattern: "[invalid", paths: [@tmpdir])
      end

      assert_match(/Grep failed/, error.message)
    end

    it "raises for a pattern spanning lines" do
      error = assert_raises(RegexpError) do
        grep_results(pattern: "first\nNeedle", paths: [@tmpdir])
      end

      assert_match(/Grep failed/, error.message)
    end

    it "raises for a pattern requiring a NUL byte" do
      error = assert_raises(RegexpError) do
        grep_results(pattern: '\x00', paths: [@tmpdir])
      end

      assert_match(/Grep failed: .*; pass `text: true` to search binary content/, error.message)
    end
  end
end
