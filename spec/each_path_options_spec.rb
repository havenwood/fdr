# frozen_string_literal: true

require_relative "spec_helper"
require "tmpdir"
require "fileutils"
require "open3"
require "pathname"

describe "Seen path options" do
  describe "depth control" do
    it "respects max_depth option" do
      shallow = path_results(paths: ["."], max_depth: 1)
      deep = path_results(paths: ["."], max_depth: 3)

      assert deep.size > shallow.size,
        "deeper search should find more files"
    end

    it "respects min_depth option" do
      results = path_results(paths: ["ext"], min_depth: 2, max_depth: 3)

      refute_empty results, "should find files at min_depth 2"
    end

    it "combines min_depth and max_depth correctly" do
      depth_2_only = path_results(paths: ["ext"], min_depth: 2, max_depth: 2)
      less_restricted = path_results(paths: ["ext"], min_depth: 2, max_depth: 3)

      assert_operator depth_2_only.size, "<=", less_restricted.size,
        "broader depth range should find at least as many files"
    end

    it "keeps directory pruning active with min_depth" do
      Dir.mktmpdir("seen-min-depth") do |dir|
        Dir.mkdir(File.join(dir, ".git"))
        File.write(File.join(dir, ".gitignore"), "ignored/\n")
        %w[.hidden ignored excluded visible].each do |directory|
          Dir.mkdir(File.join(dir, directory))
          File.write(File.join(dir, directory, "file.txt"), "")
        end

        results = path_results(
          paths: [dir],
          min_depth: 2,
          type: "f",
          exclude: %w[excluded]
        )

        assert_equal [File.join(dir, "visible", "file.txt")], results
      end
    end

    it "returns empty when min_depth exceeds directory depth" do
      results = path_results(paths: ["lib"], min_depth: 100, max_depth: 100)
      assert_empty results, "should return empty when min_depth exceeds actual depth"
    end

    it "max_depth with 1 finds top level items" do
      results = path_results(paths: ["lib"], max_depth: 1)

      refute_empty results
      assert(results.all? { |p| !p.include?("lib/seen/") },
        "max depth 1 should not include nested subdirectories")
    end
  end

  describe "path handling" do
    it "searches multiple paths" do
      results = path_results(extension: "rb", paths: %w[lib spec], max_depth: 2)

      assert(results.any? { |result| result.start_with?("lib") },
        "should find files in lib directory")
      assert(results.any? { |result| result.start_with?("spec") },
        "should find files in spec directory")
    end

    it "defaults to current directory when paths not specified" do
      results = path_results(extension: "md", max_depth: 1)

      assert(results.any? { |result| result.include?("README") },
        "should find README.md in current directory")
    end

    it "handles single path as array" do
      results = path_results(paths: ["lib"], max_depth: 1)

      refute_empty results, "should find files in lib"
      assert(results.all? { |result| result.start_with?("lib") },
        "all results should be from lib directory")
    end

    it "returns relative paths" do
      results = path_results(paths: ["lib"], max_depth: 1)

      refute_empty results
      assert(results.none? { |result| result.start_with?("/") },
        "paths should be relative, not absolute")
    end

    it "accepts binary-encoded paths" do
      results = path_results(paths: ["lib".b], max_depth: 1)

      refute_empty results
      assert(results.all? { |result| result.start_with?("lib") },
        "all results should be from lib directory")
    end

    it "accepts binary-encoded strings for every string option" do
      binary = "seen".b

      assert_equal path_results(pattern: "seen", paths: ["lib"]),
        path_results(pattern: binary, paths: ["lib"])
      assert_equal path_results(extension: "rb", paths: ["lib"]),
        path_results(extension: "rb".b, paths: ["lib"])
      assert_equal path_results(extension: "rb", paths: ["lib"]),
        path_results(extension: ["rb".b], paths: ["lib"])
      assert_equal path_results(exclude: %w[version.rb], paths: ["lib"]),
        path_results(exclude: ["version.rb".b], paths: ["lib"])
      assert_equal path_results(type: "f", paths: ["lib"]),
        path_results(type: "f".b, paths: ["lib"])
      assert_equal path_results(type: "f", paths: ["lib"]),
        path_results(type: ["f".b], paths: ["lib"])
    end

    it "raises Seen::InvalidOption for a string option that is not valid UTF-8" do
      invalid = "caf\xE9".b

      {pattern: invalid, extension: invalid, exclude: [invalid], type: invalid}.each do |key, value|
        error = assert_raises(Seen::InvalidOption) { path_results(paths: ["lib"], **{key => value}) }

        assert_kind_of Seen::Error, error
        assert_includes error.message, key.to_s
      end
    end

    it "accepts Pathname paths" do
      results = path_results(paths: [Pathname.new("lib")], max_depth: 1)

      assert_includes results, "lib/seen.rb"
    end

    it "accepts a path responding to to_path" do
      path = Object.new
      def path.to_path = "lib"

      assert_includes path_results(paths: [path], max_depth: 1), "lib/seen.rb"
    end

    it "accepts a paths value responding to to_ary" do
      paths = Object.new
      def paths.to_ary = ["lib"]

      assert_includes path_results(paths:, max_depth: 1), "lib/seen.rb"
    end

    it "limits results to specified paths only" do
      lib_only = path_results(extension: "rb", paths: ["lib"], max_depth: 1)
      spec_only = path_results(extension: "rb", paths: ["spec"], max_depth: 1)

      assert(lib_only.all? { |p| p.start_with?("lib") },
        "lib search should only return lib files")
      assert(spec_only.all? { |p| p.start_with?("spec") },
        "spec search should only return spec files")
    end

    it "does not resolve the current directory without a full-path pattern" do
      Dir.mktmpdir("seen-absolute-search") do |target|
        cwd = Dir.mktmpdir("seen-deleted-cwd")
        begin
          file = File.join(target, "match.txt")
          File.write(file, "")
          script = <<~RUBY
            require "seen"

            cwd, target = ARGV
            Dir.chdir(cwd)
            Dir.rmdir(cwd)
            Seen.each_path(paths: [target], type: "f", full_path: true, max_depth: 1).each { puts _1 }
          RUBY
          stdout, stderr, status = Open3.capture3(
            {"RUBYOPT" => nil, "RUBYLIB" => nil},
            Gem.ruby,
            "--disable-gems",
            "-I#{File.expand_path("../lib", __dir__)}",
            "-e",
            script,
            cwd,
            target
          )

          assert status.success?, stderr
          assert_equal "#{file}\n", stdout
        ensure
          FileUtils.remove_entry(cwd) if File.exist?(cwd)
        end
      end
    end
  end

  describe "exclude patterns" do
    it "filters out excluded paths" do
      all_results = path_results(extension: "toml", paths: ["ext"])
      filtered_results = path_results(
        extension: "toml",
        paths: ["ext"],
        exclude: ["ffi"]
      )

      assert filtered_results.size < all_results.size,
        "excluding ffi should reduce results"
      refute(filtered_results.any? { |result| result.include?("/ffi/") || result.start_with?("ext/ffi") },
        "results should not contain ffi directory")
    end

    it "accepts multiple exclusion patterns" do
      results = path_results(
        extension: "rs",
        paths: ["ext"],
        exclude: %w[ffi build]
      )
      refute(results.any? { |p| p.include?("/ffi/") },
        "should exclude ffi directory")
    end

    it "works with empty exclude array" do
      results_no_exclude = path_results(paths: ["lib"])
      results_empty_exclude = path_results(paths: ["lib"], exclude: [])

      assert_equal results_no_exclude.size, results_empty_exclude.size,
        "empty exclude array should not affect results"
    end

    it "actually excludes paths from results" do
      all_ext = path_results(paths: ["ext"], type: "f", extension: "toml")
      without_core = path_results(paths: ["ext"], type: "f", extension: "toml", exclude: ["core"])

      assert_operator without_core.size, "<=", all_ext.size,
        "excluding core should find fewer or equal files"
      refute(without_core.any? { |p| p.include?("/core/") || p.include?("core/") },
        "excluded core directory should not appear in results")
    end
  end

  describe "no_ignore option" do
    before do
      @tmpdir = Dir.mktmpdir("seen_ignore_test")
      Dir.mkdir(File.join(@tmpdir, ".git"))

      @normal_file = File.join(@tmpdir, "normal.txt")
      @ignored_file = File.join(@tmpdir, "ignored.txt")
      @subdir = File.join(@tmpdir, "subdir")
      @subdir_file = File.join(@subdir, "nested.txt")

      File.write(@normal_file, "normal")
      File.write(@ignored_file, "ignored")
      Dir.mkdir(@subdir)
      File.write(@subdir_file, "nested")

      gitignore = File.join(@tmpdir, ".gitignore")
      File.write(gitignore, "ignored.txt\n")
    end

    after do
      FileUtils.rm_rf(@tmpdir) if @tmpdir && File.exist?(@tmpdir)
    end

    it "respects .gitignore by default" do
      with_ignore = path_results(paths: [@tmpdir], type: "f")

      assert(with_ignore.any? { |result| result.include?("normal.txt") },
        "should find non-ignored file")
      refute(with_ignore.any? { |result| result.include?("ignored.txt") },
        "should NOT find ignored file")
    end

    it "ignores .gitignore when no_ignore is true" do
      without_ignore = path_results(paths: [@tmpdir], type: "f", no_ignore: true)

      assert(without_ignore.any? { |result| result.include?("ignored.txt") },
        "should find ignored file when no_ignore: true")
      assert(without_ignore.any? { |result| result.include?("normal.txt") },
        "should still find normal file")
    end

    it "finds more files with no_ignore than with default ignore" do
      with_ignore = path_results(paths: [@tmpdir], type: "f")
      without_ignore = path_results(paths: [@tmpdir], type: "f", no_ignore: true)

      assert without_ignore.size > with_ignore.size,
        "no_ignore should find more files than respecting .gitignore"
    end
  end

  describe "a search path named `-`" do
    # Runs in a child so the chdir cannot disturb the parallel suite.
    it "searches the file, not stdin" do
      Dir.mktmpdir("seen-dash") do |dir|
        File.write(File.join(dir, "-"), "content")
        script = "require 'seen'; print Seen.each_path(paths: ['-']).to_a.sort.inspect"
        lib = File.expand_path("../lib", __dir__)
        output, status = Open3.capture2(Gem.ruby, "-I#{lib}", "-e", script, chdir: dir)

        assert_predicate status, :success?
        assert_equal ["./-"].inspect, output
      end
    end
  end
end
