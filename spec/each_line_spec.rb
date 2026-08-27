# frozen_string_literal: true

require_relative "spec_helper"
require "fileutils"
require "tmpdir"

describe "Seen.each_line" do
  before do
    @tmpdir = Dir.mktmpdir("seen_line_test")
    @path = File.join(@tmpdir, "example.rb")
    File.write(@path, "first\nNeedle\nneedle\nneedle twice needle\n")
  end

  after do
    FileUtils.rm_rf(@tmpdir) if @tmpdir && File.exist?(@tmpdir)
  end

  describe "results" do
    it "returns an Enumerator of path, one-based line number and text" do
      results = Seen.each_line(pattern: "needle", paths: [@tmpdir])

      assert_kind_of Enumerator, results
      assert_equal [[@path, 3, "needle"], [@path, 4, "needle twice needle"]], results.to_a.sort
    end

    it "yields to a block and returns the Enumerator" do
      yielded = []

      results = Seen.each_line(pattern: "needle", paths: [@tmpdir]) { |*match| yielded << match }

      assert_kind_of Enumerator, results
      assert_equal results.to_a.sort, yielded.sort
    end

    it "groups matches by path" do
      results = Seen.each_line(pattern: "needle", paths: [@tmpdir]).group_by { |path,| path }

      assert_equal [[@path, 3, "needle"], [@path, 4, "needle twice needle"]], results[@path].sort
    end

    it "leaves a location-keyed Hash to the caller, which dedupes" do
      results = Seen.each_line(pattern: "needle", paths: [@path, @path])
        .to_h { |path, line_number, text| [[path, line_number], text] }

      assert_equal({[@path, 3] => "needle", [@path, 4] => "needle twice needle"}, results)
    end

    it "reports a line with two matches once" do
      results = Seen.each_line(pattern: "needle", paths: [@tmpdir]).to_a

      assert_equal [[@path, 4, "needle twice needle"]], results.filter { |_path, line_number, _text| line_number == 4 }
    end

    it "omits the ./ prefix when paths defaults, as rg does" do
      path, = Seen.each_line(pattern: "module Seen", name: '^version\.rb$').first

      assert_equal "lib/seen/version.rb", path
    end

    it "reports one match per occurrence with column, as rg --vimgrep does" do
      results = Seen.each_line(pattern: "needle", paths: [@tmpdir], column: true).to_a.sort

      assert_equal [[@path, 3, 1, "needle"],
        [@path, 4, 1, "needle twice needle"],
        [@path, 4, 14, "needle twice needle"]], results
    end

    it "reports a byte Range for String#byteslice" do
      results = Seen.each_line(pattern: "needle", paths: [@tmpdir], byte_range: true)
        .sort_by { |path, line_number, range, _text| [path, line_number, range.begin] }

      assert_equal [[@path, 3, 0...6, "needle"],
        [@path, 4, 0...6, "needle twice needle"],
        [@path, 4, 13...19, "needle twice needle"]], results
      results.each do |_path, _line_number, range, text|
        assert_equal "needle", text.byteslice(range)
      end
    end

    it "counts the column in bytes, as rg does" do
      path = File.join(@tmpdir, "multibyte.txt")
      File.write(path, "caf\u00e9 needle here\n")

      _, _, column, text = Seen.each_line(pattern: "needle", paths: [path], column: true).first

      assert_equal 7, column
      assert_equal "needle", text.byteslice(column - 1, 6)
    end

    it "indexes the original CRLF line in occurrence modes" do
      path = File.join(@tmpdir, "crlf.txt")
      File.binwrite(path, "foo\r\n")

      assert_equal [[path, 1, 4, "foo\r"]],
        Seen.each_line(pattern: '\\r', paths: [path], column: true).to_a

      _, _, range, text = Seen.each_line(pattern: '\\r', paths: [path], byte_range: true).first
      assert_equal [3...4, "\r"], [range, text.byteslice(range)]

      _, _, range, text = Seen.each_line(pattern: "$", paths: [path], byte_range: true).first
      assert_equal [4...4, ""], [range, text.byteslice(range)]
    end

    it "freezes the line in every result shape" do
      path = File.join(@tmpdir, "shared.txt")
      File.write(path, "needle needle needle needle needle\n")

      [{}, {column: true}, {byte_range: true}].each do |shape|
        texts = Seen.each_line(pattern: "needle", paths: [path], **shape).map { |*match| match.last }

        assert_equal(shape.empty? ? 1 : 5, texts.size)
        assert(texts.all?(&:frozen?), "result text must not be mutable")
        next if shape.empty?

        assert_equal 1, texts.map(&:object_id).uniq.size,
          "every occurrence on a line should share its text"
      end
    end

    it "keeps shared occurrence text across GC compaction" do
      path = File.join(@tmpdir, "shared.txt")
      File.write(path, "needle needle\n")
      matches = Seen.each_line(pattern: "needle", paths: [path], column: true)

      first = matches.next.last
      GC.compact
      second = matches.next.last

      assert_same first, second
    end

    it "caps matching lines per file with max_count, as rg -m does" do
      other = File.join(@tmpdir, "other.rb")
      File.write(other, "needle\nneedle\nneedle\n")

      capped = line_results(pattern: "needle", paths: [@tmpdir], max_count: 1)

      assert_equal [1], capped[other].keys
      assert_equal 1, capped[@path].size, "the cap is per file, not per search"
    end

    it "bounds the per-worker search buffer with heap_limit" do
      path = File.join(@tmpdir, "long-line.txt")
      File.write(path, "needle #{"x" * 256}\n")

      assert_empty Seen.each_line(pattern: "needle", paths: [path], heap_limit: 64).to_a
      error = assert_raises(Seen::IOError) do
        Seen.each_line(
          pattern: "needle",
          paths: [path],
          heap_limit: 64,
          ignore_error: false
        ).to_a
      end
      assert_match(/configured allocation limit/, error.message)
    end

    it "does not walk or open files when max_count is zero" do
      missing = File.join(@tmpdir, "missing")

      assert_empty Seen.each_line(
        pattern: "needle",
        paths: [missing],
        max_count: 0,
        ignore_error: false
      ).to_a
    end

    it "detects UTF-16 byte order marks by default, as rg does" do
      path = File.join(@tmpdir, "utf16.txt")
      File.binwrite(path, "\xFF\xFE".b + "needle wide\n".encode("UTF-16LE").b)

      assert_equal [[path, 1, "needle wide"]],
        Seen.each_line(pattern: "needle", paths: [path]).to_a
      assert_equal [[path, 1, "needle wide"]],
        Seen.each_line(pattern: "needle", paths: [path], encoding: "auto").to_a
    end

    it "reads a named encoding with encoding, as rg -E does" do
      path = File.join(@tmpdir, "utf16.txt")
      File.binwrite(path, "needle wide\n".encode("UTF-16LE").b)

      assert_empty Seen.each_line(pattern: "needle", paths: [path]).to_a
      assert_equal [[path, 1, "needle wide"]],
        Seen.each_line(pattern: "needle", paths: [path], encoding: "utf-16le").to_a
    end

    it "lets a BOM override encoding, as rg -E does" do
      path = File.join(@tmpdir, "utf16be.txt")
      File.binwrite(path, "\xFE\xFF".b + "needle wide\n".encode("UTF-16BE").b)

      assert_equal [[path, 1, "needle wide"]],
        Seen.each_line(pattern: "needle", paths: [path], encoding: "utf-16le").to_a
    end

    it "tags explicitly decoded lines as UTF-8" do
      path = File.join(@tmpdir, "utf16.txt")
      File.binwrite(path, "\xFF\xFE".b + "needle café\n".encode("UTF-16LE").b)
      original_external = Encoding.default_external
      original_verbose = $VERBOSE
      $VERBOSE = nil
      Encoding.default_external = Encoding::ISO_8859_1
      $VERBOSE = original_verbose

      line = Seen.each_line(pattern: "needle", paths: [path], encoding: "utf-16le").first.last

      assert_equal Encoding::UTF_8, line.encoding
      assert_equal "needle café", line
      assert line.frozen?
    ensure
      $VERBOSE = nil
      Encoding.default_external = original_external if original_external
      $VERBOSE = original_verbose
    end

    it "tags automatically decoded lines as UTF-8" do
      path = File.join(@tmpdir, "utf16.txt")
      File.binwrite(path, "\xFF\xFE".b + "needle café\n".encode("UTF-16LE").b)
      original_external = Encoding.default_external
      original_verbose = $VERBOSE
      $VERBOSE = nil
      Encoding.default_external = Encoding::ISO_8859_1
      $VERBOSE = original_verbose

      line = Seen.each_line(pattern: "needle", paths: [path]).first.last

      assert_equal Encoding::UTF_8, line.encoding
      assert_equal "needle café", line
      assert line.frozen?
    ensure
      $VERBOSE = nil
      Encoding.default_external = original_external if original_external
      $VERBOSE = original_verbose
    end

    it "searches raw bytes with encoding none" do
      path = File.join(@tmpdir, "utf8.txt")
      File.binwrite(path, "\xEF\xBB\xBFneedle\n".b)

      assert_empty Seen.each_line(pattern: "^needle", paths: [path], encoding: "none").to_a
      _, _, line = Seen.each_line(pattern: "needle", paths: [path], encoding: "none").first
      assert_equal "\xEF\xBB\xBFneedle".b, line.b
      assert_equal Encoding.default_external, line.encoding
    end

    it "rejects an unknown encoding" do
      error = assert_raises(Seen::InvalidOption) do
        Seen.each_line(pattern: "needle", paths: [@tmpdir], encoding: "nope-9").first
      end

      assert_match(/unknown encoding/, error.message)
    end

    it "rejects an unknown encoding before empty result shortcuts" do
      [{paths: []}, {paths: [@tmpdir], min_depth: 2, max_depth: 1}, {max_count: 0}].each do |options|
        error = assert_raises(Seen::InvalidOption) do
          Seen.each_line(pattern: "needle", encoding: "nope-9", **options).first
        end

        assert_match(/unknown encoding/, error.message)
      end
    end

    it "yields nothing when nothing matches" do
      results = Seen.each_line(pattern: "haystack", paths: [@tmpdir])

      assert_empty results.to_a
    end

    it "starts grepping when the Enumerator is consumed" do
      results = Seen.each_line(pattern: "later", paths: [@tmpdir])
      path = File.join(@tmpdir, "later.txt")
      File.write(path, "later\n")

      assert_includes results.to_a, [path, 1, "later"]
    end

    it "can be enumerated again" do
      results = Seen.each_line(pattern: "needle", paths: [@tmpdir])
      first = results.to_a.sort

      assert_equal first, results.to_a.sort
    end

    it "emits matches again for a repeated path" do
      results = Seen.each_line(pattern: "needle", paths: [@path, @path]).to_a

      assert_equal 2, results.count { |match| match == [@path, 3, "needle"] }
      assert_equal 2, results.count { |match| match == [@path, 4, "needle twice needle"] }
    end

    it "tags matching lines with the external encoding" do
      _, _, line = Seen.each_line(pattern: "needle", paths: [@tmpdir]).find do |_path, line_number, _text|
        line_number == 3
      end

      assert_equal Encoding.default_external, line.encoding
    end
  end

  describe "content matching" do
    it "is case sensitive by default" do
      results = line_results(pattern: "needle", paths: [@tmpdir])

      assert_equal [3, 4], results[@path].keys
    end

    it "can search case insensitively" do
      results = line_results(pattern: "needle", paths: [@tmpdir], content_case_sensitive: false)

      assert_equal [2, 3, 4], results[@path].keys
    end

    it "treats nil case sensitivity as falsey" do
      results = line_results(pattern: "needle", paths: [@tmpdir], content_case_sensitive: nil)

      assert_equal [2, 3, 4], results[@path].keys
    end

    it "treats a non-false byte_range value as truthy" do
      result = Seen.each_line(pattern: "needle", paths: [@path], byte_range: "yes").first

      assert_equal [@path, 3, 0...6, "needle"], result
    end

    it "scans binary files when text is true, as rg -a does" do
      binary = File.join(@tmpdir, "binary.bin")
      File.binwrite(binary, "before\0needle here\n")

      assert_empty line_results(pattern: "needle here", paths: [binary])
      assert_equal({binary => {1 => "before\0needle here"}},
        line_results(pattern: "needle here", paths: [binary], text: true))
    end

    it "allows a NUL in the pattern only when text is true" do
      binary = File.join(@tmpdir, "binary.bin")
      File.binwrite(binary, "before\0needle here\n")

      [{}, {column: true}, {byte_range: true}].each do |shape|
        assert_empty Seen.each_line(pattern: "e\0n", paths: [binary], **shape).to_a
        refute_empty Seen.each_line(pattern: "e\0n", paths: [binary], text: true, **shape).to_a
      end
    end

    it "skips binary files" do
      File.binwrite(File.join(@tmpdir, "binary.bin"), "needle\n\0needle\n")

      results = line_results(pattern: "needle", paths: [@tmpdir])

      assert_equal [@path], results.keys
    end
  end

  describe "file selection" do
    it "skips hidden files by default" do
      File.write(File.join(@tmpdir, ".hidden.rb"), "needle\n")

      default_results = line_results(pattern: "needle", paths: [@tmpdir])
      with_hidden = line_results(pattern: "needle", paths: [@tmpdir], hidden: true)

      assert_equal [@path], default_results.keys
      assert_equal 2, with_hidden.size
    end

    it "respects gitignore by default" do
      Dir.mkdir(File.join(@tmpdir, ".git"))
      File.write(File.join(@tmpdir, ".gitignore"), "ignored.rb\n")
      File.write(File.join(@tmpdir, "ignored.rb"), "needle\n")

      default_results = line_results(pattern: "needle", paths: [@tmpdir])
      without_ignore = line_results(pattern: "needle", paths: [@tmpdir], no_ignore: true)

      assert_equal [@path], default_results.keys
      assert_equal 2, without_ignore.size
    end

    it "skips excluded patterns" do
      Dir.mkdir(File.join(@tmpdir, "vendor"))
      File.write(File.join(@tmpdir, "vendor", "skip.rb"), "needle\n")

      results = line_results(pattern: "needle", paths: [@tmpdir], exclude: %w[vendor])

      assert_equal [@path], results.keys
    end

    it "searches every given path" do
      other = Dir.mktmpdir("seen_line_other")
      other_path = File.join(other, "other.rb")
      File.write(other_path, "needle\n")

      results = line_results(pattern: "needle", paths: [@tmpdir, other])

      assert_equal [@path, other_path].sort, results.keys.sort
    ensure
      FileUtils.rm_rf(other)
    end

    it "returns no results for an empty paths array" do
      assert_empty line_results(pattern: "needle", paths: [])
    end

    it "rejects nil paths" do
      assert_raises(TypeError) { line_results(pattern: "needle", paths: nil) }
    end

    it "filters by size" do
      big = File.join(@tmpdir, "big.txt")
      File.write(big, "needle\n#{"padding\n" * 100}")

      results = line_results(pattern: "needle", paths: [@tmpdir], max_size: 100)

      assert_includes results.keys, @path
      refute_includes results.keys, big
    end

    it "filters by modification time" do
      old = File.join(@tmpdir, "old.txt")
      File.write(old, "needle\n")
      File.utime(Time.now - 86_400, Time.now - 86_400, old)

      results = line_results(pattern: "needle", paths: [@tmpdir], changed_within: 3600)

      assert_includes results.keys, @path
      refute_includes results.keys, old
    end

    it "filters by extension" do
      File.write(File.join(@tmpdir, "notes.txt"), "needle\n")

      results = line_results(pattern: "needle", paths: [@tmpdir], extension: "rb")

      assert_equal [@path], results.keys
    end

    it "filters by multiple extensions" do
      notes = File.join(@tmpdir, "notes.txt")
      File.write(notes, "needle\n")

      results = line_results(pattern: "needle", paths: [@tmpdir], extension: %w[rb txt])

      assert_equal [@path, notes].sort, results.keys.sort
    end

    it "filters by filename with name" do
      File.write(File.join(@tmpdir, "example_spec.rb"), "needle\n")

      results = line_results(pattern: "needle", paths: [@tmpdir], name: '_spec\.rb$')

      assert_equal [File.join(@tmpdir, "example_spec.rb")], results.keys
    end

    it "filters names case insensitively by default" do
      upper_path = File.join(@tmpdir, "UPPER.rb")
      File.write(upper_path, "needle\n")

      default_results = line_results(pattern: "needle", paths: [@tmpdir], name: "upper")
      sensitive = line_results(pattern: "needle", paths: [@tmpdir], name: "upper", case_sensitive: true)

      assert_equal [upper_path], default_results.keys
      assert_empty sensitive
    end

    it "filters names with a glob when glob is true" do
      File.write(File.join(@tmpdir, "notes.txt"), "needle\n")

      results = line_results(pattern: "needle", paths: [@tmpdir], name: "*.rb", glob: true)

      assert_equal [@path], results.keys
    end

    it "respects max_depth" do
      nested = File.join(@tmpdir, "nested")
      Dir.mkdir(nested)
      File.write(File.join(nested, "deep.rb"), "needle\n")

      results = line_results(pattern: "needle", paths: [@tmpdir], max_depth: 1)

      assert_equal [@path], results.keys
    end

    it "keeps directory pruning active with min_depth" do
      Dir.mkdir(File.join(@tmpdir, ".git"))
      File.write(File.join(@tmpdir, ".gitignore"), "ignored/\n")
      %w[.hidden ignored excluded visible].each do |directory|
        Dir.mkdir(File.join(@tmpdir, directory))
        File.write(File.join(@tmpdir, directory, "match.rb"), "needle\n")
      end

      results = line_results(
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
      assert_raises(ArgumentError) { line_results(paths: [@tmpdir]) }
    end

    it "rejects the search-only type filter" do
      assert_raises(ArgumentError) do
        line_results(pattern: "needle", paths: [@tmpdir], type: "f")
      end
    end

    it "rejects column and byte_range together" do
      error = assert_raises(Seen::InvalidOption) do
        Seen.each_line(pattern: "needle", paths: [@tmpdir], column: true, byte_range: true)
      end

      assert_match(/column and byte_range cannot both be true/, error.message)
    end

    it "raises for an invalid regex pattern" do
      error = assert_raises(RegexpError) do
        line_results(pattern: "[invalid", paths: [@tmpdir])
      end

      assert_match(/Line search failed/, error.message)
    end

    it "raises for a pattern spanning lines" do
      error = assert_raises(RegexpError) do
        line_results(pattern: "first\nNeedle", paths: [@tmpdir])
      end

      assert_match(/Line search failed/, error.message)
    end

    it "raises for a pattern requiring a NUL byte" do
      error = assert_raises(RegexpError) do
        line_results(pattern: '\x00', paths: [@tmpdir])
      end

      assert_match(/Line search failed: .*; pass `text: true` to search binary content/, error.message)
    end
  end
end
