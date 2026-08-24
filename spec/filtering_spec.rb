# frozen_string_literal: true

require_relative "spec_helper"
require "fileutils"
require "tmpdir"

describe "Fdr filtering" do
  describe "extension filtering" do
    it "filters by single extension" do
      results = Fdr.search(extension: "toml", paths: ["ext"], max_depth: 3)
      refute_empty results
      assert(results.all? { |result| result.end_with?(".toml") })
    end

    it "finds rb files" do
      results = Fdr.search(extension: "rb", paths: ["lib"])
      assert(results.all? { |result| result.end_with?(".rb") })
      assert(results.any? { |result| result.include?("fdr.rb") })
    end

    it "works with extension without leading dot" do
      results = Fdr.search(extension: "rb", paths: ["lib"], max_depth: 2)
      refute_empty results
      assert(results.all? { |result| result.end_with?(".rb") })
    end

    it "accepts extension arrays" do
      Dir.mktmpdir("fdr-extensions") do |dir|
        %w[one.rb two.rs three.txt].each { File.write(File.join(dir, _1), "") }

        all = Fdr.search(paths: [dir], type: "f").sort
        expected = %w[one.rb two.rs].map { File.join(dir, _1) }.sort

        assert_equal expected, Fdr.search(paths: [dir], extension: [".rb", "rs"]).sort
        assert_equal Fdr.search(paths: [dir], extension: "rb"),
          Fdr.search(paths: [dir], extension: ["rb"])
        assert_equal all, Fdr.search(paths: [dir], extension: []).sort
      end
    end
  end

  describe "file type filtering" do
    it "finds files only with type f" do
      results = Fdr.search(type: "f", paths: ["lib"], max_depth: 2)
      refute_empty results
    end

    it "finds directories only with type d" do
      results = Fdr.search(type: "d", paths: ["."], max_depth: 2)
      refute_empty results
      assert(results.all? { |path| File.directory?(path) })
    end

    it "excludes directories when type is f" do
      results = Fdr.search(type: "f", paths: ["."], max_depth: 1)
      assert(results.none? { |path| File.directory?(path) })
    end

    it "supports file type alias" do
      results = Fdr.search(type: "file", paths: ["lib"], max_depth: 2)
      refute_empty results
      assert(results.all? { |path| File.file?(path) })
    end

    it "accepts file types as symbols" do
      results = Fdr.search(type: :file, paths: ["lib"], max_depth: 2)

      refute_empty results
      assert(results.all? { |path| File.file?(path) })
    end

    it "supports dir type alias" do
      results = Fdr.search(type: "dir", paths: ["."], max_depth: 2)
      refute_empty results
      assert(results.all? { |path| File.directory?(path) })
    end

    it "supports directory type alias" do
      results = Fdr.search(type: "directory", paths: ["."], max_depth: 2)
      refute_empty results
      assert(results.all? { |path| File.directory?(path) })
    end

    it "accepts file type arrays" do
      Dir.mktmpdir("fdr-file-types") do |dir|
        File.write(File.join(dir, "file"), "")
        Dir.mkdir(File.join(dir, "directory"))

        all = Fdr.search(paths: [dir], max_depth: 1).sort

        assert_equal all, Fdr.search(paths: [dir], type: [:f, "directory"], max_depth: 1).sort
        assert_equal Fdr.search(paths: [dir], type: :f),
          Fdr.search(paths: [dir], type: [:f])
        assert_equal all, Fdr.search(paths: [dir], type: [], max_depth: 1).sort
      end
    end
  end

  describe "ignore files" do
    def ignore_tree
      Dir.mktmpdir("fdr-ignore") do |dir|
        # `.gitignore` only applies inside a repository, unlike the others.
        Dir.mkdir(File.join(dir, ".git"))
        File.write(File.join(dir, ".gitignore"), "git.txt\n")
        File.write(File.join(dir, ".ignore"), "plain.txt\n")
        File.write(File.join(dir, ".fdignore"), "fd.txt\n")
        File.write(File.join(dir, ".rgignore"), "rg.txt\n")
        %w[git.txt plain.txt fd.txt rg.txt keep.txt].each do |name|
          File.write(File.join(dir, name), "needle\n")
        end
        yield dir
      end
    end

    def basenames(paths) = paths.map { File.basename(_1) }.sort

    it "honors .fdignore for search and .rgignore for grep" do
      ignore_tree do |dir|
        assert_equal %w[keep.txt rg.txt], basenames(Fdr.search(paths: [dir], type: "f"))
        assert_equal %w[fd.txt keep.txt], basenames(Fdr.grep(pattern: "needle", paths: [dir]).keys)
      end
    end

    it "honors .gitignore and .ignore for both" do
      ignore_tree do |dir|
        found = basenames(Fdr.search(paths: [dir], type: "f")) +
          basenames(Fdr.grep(pattern: "needle", paths: [dir]).keys)

        refute_includes found, "git.txt"
        refute_includes found, "plain.txt"
      end
    end

    it "disables every ignore file with no_ignore" do
      ignore_tree do |dir|
        results = basenames(Fdr.search(paths: [dir], type: "f", no_ignore: true, hidden: true))

        assert_equal %w[.fdignore .gitignore .ignore .rgignore fd.txt git.txt keep.txt
          plain.txt rg.txt], results
      end
    end
  end

  describe "exclude globs" do
    # `ignore` supports a single override root, so a slash-containing glob
    # anchors to the first path only. `fd -E` behaves the same way.
    it "anchors a slash-containing exclude to the first path" do
      Dir.mktmpdir("fdr-exclude-roots") do |dir|
        %w[a b].each do |root|
          FileUtils.mkdir_p(File.join(dir, root, "sub", "vendor"))
          File.write(File.join(dir, root, "sub", "vendor", "x.rb"), "")
        end

        results = Fdr.search(
          paths: [File.join(dir, "a"), File.join(dir, "b")],
          exclude: ["sub/vendor"]
        )

        assert(results.none? { |path| path.include?("/a/sub/vendor") },
          "the first path should honor the slash-containing exclude")
        assert(results.any? { |path| path.include?("/b/sub/vendor") },
          "later paths keep fd's behavior of ignoring the anchored exclude")
      end
    end
  end

  describe "hidden files" do
    it "excludes hidden files by default" do
      results = Fdr.search(paths: ["."], max_depth: 1)
      hidden_files = results.select do |result|
        basename = File.basename(result)
        basename.start_with?(".") && basename != "."
      end
      assert_empty hidden_files
    end

    it "includes hidden files when requested" do
      results = Fdr.search(paths: ["."], max_depth: 1, hidden: true)
      assert(results.any? { |result| File.basename(result).start_with?(".") })
    end

    it "finds dotfiles with hidden option" do
      results = Fdr.search(pattern: "gitignore", paths: ["."], hidden: true, max_depth: 1)
      assert(results.any? { |result| result.include?(".gitignore") })
    end
  end
end
