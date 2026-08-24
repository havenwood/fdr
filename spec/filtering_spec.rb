# frozen_string_literal: true

require_relative "spec_helper"
require "fileutils"
require "open3"
require "tmpdir"

describe "Fdr filtering" do
  describe "ignore_file" do
    def with_ignore_file
      Dir.mktmpdir("fdr-ignore-file") do |dir|
        File.write(File.join(dir, "custom.ignore"), "skipme.txt\n")
        File.write(File.join(dir, "skipme.txt"), "needle\n")
        File.write(File.join(dir, "keep.txt"), "needle\n")
        yield dir, File.join(dir, "custom.ignore")
      end
    end

    it "applies an extra gitignore-format file" do
      with_ignore_file do |dir, ignore|
        assert_includes Fdr.search(paths: [dir], type: "f"), File.join(dir, "skipme.txt")
        refute_includes Fdr.search(paths: [dir], type: "f", ignore_file: [ignore]),
          File.join(dir, "skipme.txt")
      end
    end

    it "outranks no_ignore, as fd and rg do" do
      with_ignore_file do |dir, ignore|
        results = Fdr.search(paths: [dir], type: "f", ignore_file: [ignore], no_ignore: true)

        refute_includes results, File.join(dir, "skipme.txt")
        assert_includes results, File.join(dir, "keep.txt")
      end
    end

    it "applies to grep too" do
      with_ignore_file do |dir, ignore|
        results = Fdr.grep(pattern: "needle", paths: [dir], ignore_file: [ignore])

        assert_equal [File.join(dir, "keep.txt")], results.keys
      end
    end

    it "skips a missing ignore file unless ignore_error is false" do
      with_ignore_file do |dir, _ignore|
        missing = File.join(dir, "nope.ignore")

        refute_empty Fdr.search(paths: [dir], type: "f", ignore_file: [missing])
        assert_raises(Fdr::IOError) do
          Fdr.search(paths: [dir], ignore_file: [missing], ignore_error: false).to_a
        end
      end
    end

    it "classifies a malformed ignore-file glob as an invalid option" do
      with_ignore_file do |dir, ignore|
        File.write(ignore, "skipme.txt\n[z-a]\n")

        refute_includes Fdr.search(paths: [dir], type: "f", ignore_file: [ignore]),
          File.join(dir, "skipme.txt")
        error = assert_raises(Fdr::InvalidOption) do
          Fdr.search(paths: [dir], ignore_file: [ignore], ignore_error: false).to_a
        end

        assert_match(/invalid range/, error.message)
      end
    end
  end

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

    def global_ignore_output(tree, environment, chdir: Dir.pwd)
      script = <<~RUBY
        require "fdr"
        tree = ARGV[0]
        names = ->(paths) { paths.map { File.basename(_1) }.sort }
        print [names[Fdr.search(paths: [tree], type: "f")],
               names[Fdr.grep(pattern: "needle", paths: [tree]).keys]].inspect
      RUBY
      output, status = Open3.capture2(
        environment,
        Gem.ruby, "-I#{File.expand_path("../lib", __dir__)}", "-e", script, tree,
        chdir:
      )

      assert_predicate status, :success?
      output
    end

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

    # Out of process because XDG_CONFIG_HOME is global and the suite is parallel.
    it "honors fd's global ignore file for search only" do
      Dir.mktmpdir("fdr-global-ignore") do |dir|
        tree = File.join(dir, "tree")
        config = File.join(dir, "config")
        FileUtils.mkdir_p(File.join(config, "fd"))
        Dir.mkdir(tree)
        File.write(File.join(config, "fd", "ignore"), "global.txt\n")
        %w[global.txt keep.txt].each { File.write(File.join(tree, _1), "needle\n") }

        output = global_ignore_output(tree, {"XDG_CONFIG_HOME" => config})

        assert_equal [%w[keep.txt], %w[global.txt keep.txt]].inspect, output
      end
    end

    it "falls back to HOME when XDG_CONFIG_HOME is empty or relative" do
      Dir.mktmpdir("fdr-global-ignore") do |dir|
        tree = File.join(dir, "tree")
        home = File.join(dir, "home")
        FileUtils.mkdir_p([tree, File.join(home, ".config", "fd")])
        File.write(File.join(home, ".config", "fd", "ignore"), "global.txt\n")
        %w[global.txt keep.txt local.txt].each { File.write(File.join(tree, _1), "needle\n") }

        ["", "relative"].each do |xdg|
          config = xdg.empty? ? dir : File.join(dir, xdg)
          FileUtils.mkdir_p(File.join(config, "fd"))
          File.write(File.join(config, "fd", "ignore"), "local.txt\n")

          output = global_ignore_output(
            tree,
            {"XDG_CONFIG_HOME" => xdg, "HOME" => home},
            chdir: dir
          )

          assert_equal [%w[keep.txt local.txt], %w[global.txt keep.txt local.txt]].inspect, output
        end
      end
    end

    it "ignores empty or relative HOME when XDG_CONFIG_HOME is unset" do
      Dir.mktmpdir("fdr-global-ignore") do |dir|
        tree = File.join(dir, "tree")
        Dir.mkdir(tree)
        %w[keep.txt local.txt].each { File.write(File.join(tree, _1), "needle\n") }

        ["", "relative"].each do |home|
          config = home.empty? ? dir : File.join(dir, home)
          FileUtils.mkdir_p(File.join(config, ".config", "fd"))
          File.write(File.join(config, ".config", "fd", "ignore"), "local.txt\n")

          output = global_ignore_output(
            tree,
            {"XDG_CONFIG_HOME" => nil, "HOME" => home},
            chdir: dir
          )

          assert_equal [%w[keep.txt local.txt], %w[keep.txt local.txt]].inspect, output
        end
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
