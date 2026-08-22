# frozen_string_literal: true

require_relative "spec_helper"
require "tmpdir"
require "fileutils"
require "timeout"

def create_gitignore_and_ignored_dir(tmpdir)
  ignored_dir = File.join(tmpdir, "ignored_dir")
  ignored_file = File.join(ignored_dir, "file.txt")
  link_to_ignored = File.join(tmpdir, "link_to_ignored")

  Dir.mkdir(ignored_dir)
  File.write(ignored_file, "content")
  File.symlink(ignored_dir, link_to_ignored)

  gitignore = File.join(tmpdir, ".gitignore")
  File.write(gitignore, "ignored_dir/\n")

  {ignored_dir:, ignored_file:, link_to_ignored:}
end

describe "Symlink behavior" do
  before do
    @tmpdir = Dir.mktmpdir("fdr_symlink_test")

    @real_dir = File.join(@tmpdir, "real_dir")
    Dir.mkdir(@real_dir)

    @file1 = File.join(@real_dir, "file1.txt")
    @file2 = File.join(@real_dir, "file2.rb")
    @real_file = File.join(@tmpdir, "real_file.txt")

    File.write(@file1, "content1")
    File.write(@file2, "content2")
    File.write(@real_file, "content3")

    @link_to_dir = File.join(@tmpdir, "link_to_dir")
    @link_to_file = File.join(@tmpdir, "link_to_file")
    @broken_link = File.join(@tmpdir, "broken_link")

    File.symlink(@real_dir, @link_to_dir)
    File.symlink(@real_file, @link_to_file)
    File.symlink("nonexistent", @broken_link)
  end

  after do
    FileUtils.rm_rf(@tmpdir) if @tmpdir && File.exist?(@tmpdir)
  end

  describe "follow: false (default)" do
    it "does not traverse into symlinked directories" do
      results = Fdr.search(paths: [@tmpdir], follow: false)

      assert(results.any? { |path| path.include?("real_dir") })

      real_dir_entries = results.select { |path| path.include?("file1.txt") }
      assert_equal 1, real_dir_entries.size, "should only find file1.txt once (not through symlink)"
    end

    it "finds symlinks themselves when type is not specified" do
      results = Fdr.search(paths: [@tmpdir], follow: false, max_depth: 1)

      assert_includes results, @link_to_dir
    end

    it "does not follow symlinked files" do
      results = Fdr.search(paths: [@tmpdir], pattern: "link_to_file", follow: false)

      refute_empty results
      assert(results.all? { |path| File.symlink?(path) })
    end
  end

  describe "follow: true" do
    it "traverses into symlinked directories" do
      results = Fdr.search(paths: [@tmpdir], follow: true)

      assert(results.any? { |path| path.include?("real_dir") && path.include?("file1.txt") })

      txt_files = results.select { |path| path.end_with?(".txt") }
      assert txt_files.size >= 2, "should find at least real_file.txt and file1.txt"
    end

    it "follows symlinks to files" do
      results = Fdr.search(paths: [@tmpdir], pattern: "real_file", follow: true)

      refute_empty results
    end

    it "lists broken symlinks, as in fd" do
      results = Fdr.search(paths: [@tmpdir], follow: true)

      assert_includes results, @broken_link
    end
  end

  describe 'type: "l" (symlink)' do
    it "finds only symlinks" do
      results = Fdr.search(paths: [@tmpdir], type: "l", max_depth: 1)

      refute_empty results
      assert results.all? { |path| File.symlink?(path) }, "all results should be symlinks"
    end

    it "finds directory symlinks" do
      results = Fdr.search(paths: [@tmpdir], type: "l", pattern: "link_to_dir", max_depth: 1)

      refute_empty results
      assert(results.any? do |path|
        File.symlink?(path) && File.directory?(File.readlink(path).start_with?("/") ? File.readlink(path) : File.join(
          File.dirname(path), File.readlink(path)
        ))
      end)
    end

    it "finds file symlinks" do
      results = Fdr.search(paths: [@tmpdir], type: "l", pattern: "link_to_file", max_depth: 1)

      refute_empty results
      assert(results.all? { |path| File.symlink?(path) })
    end

    it "finds broken symlinks" do
      results = Fdr.search(paths: [@tmpdir], type: "l", max_depth: 1)

      assert results.size >= 3, "should find at least 3 symlinks, found #{results.size}"
      assert results.any? { |path| File.symlink?(path) && !File.exist?(path) }, "should include broken symlink"
    end

    it "works with symlink type alias" do
      results = Fdr.search(paths: [@tmpdir], type: "symlink", max_depth: 1)

      refute_empty results
      assert(results.all? { |path| File.symlink?(path) })
    end
  end

  describe "symlink as the search root" do
    it "omits the root link and returns its children" do
      results = Fdr.search(paths: [@link_to_dir])

      refute_includes results, @link_to_dir
      assert(results.any? { |path| path.include?("file1.txt") })
      assert(results.any? { |path| path.include?("file2.rb") })
    end

    it "omits the root link and returns its children with follow: true" do
      results = Fdr.search(paths: [@link_to_dir], follow: true)

      refute_includes results, @link_to_dir
      assert(results.any? { |path| path.include?("file1.txt") })
    end

    it 'finds a file symlink root with type: "l"' do
      results = Fdr.search(paths: [@link_to_file], type: "l")

      assert_equal [@link_to_file], results
    end

    it "sizes a file symlink root by the link, not its target" do
      big = File.join(@tmpdir, "big.txt")
      link = File.join(@tmpdir, "big_link")
      File.write(big, "x" * 21_000)
      File.symlink(big, link)

      assert_empty Fdr.search(paths: [link], min_size: 20_000)
      assert_equal [link], Fdr.search(paths: [link], min_size: 20_000, follow: true)
    end

    it "answers the same for a symlink root on both sides of the parallel threshold" do
      big = File.join(@tmpdir, "big.txt")
      link = File.join(@tmpdir, "big_link")
      pad = File.join(@tmpdir, "pad")
      File.write(big, "x" * 21_000)
      File.symlink(big, link)
      Dir.mkdir(pad)

      serial = Fdr.search(paths: [pad, link], min_size: 20_000, follow: true)
      1_200.times { |i| File.write(File.join(pad, "f#{i}"), "") }
      parallel = Fdr.search(paths: [pad, link], min_size: 20_000, follow: true)

      assert_equal [link], serial
      assert_equal [link], parallel - Fdr.search(paths: [pad], min_size: 20_000, follow: true)
    end
  end


  describe "combination of follow and type" do
    it 'type: "l" with follow: true matches only broken symlinks' do
      results = Fdr.search(paths: [@tmpdir], type: "l", follow: true, max_depth: 1)

      assert_equal [@broken_link], results,
        "followed symlinks take their target type, so only broken links remain"
    end

    it 'type: "l" with follow: false finds symlinks' do
      results = Fdr.search(paths: [@tmpdir], type: "l", follow: false, max_depth: 1)

      refute_empty results
      assert(results.all? { |path| File.symlink?(path) })
    end

    it 'type: "f" excludes symlinks with follow: false' do
      results = Fdr.search(paths: [@tmpdir], type: "f", follow: false)

      assert(results.all? { |path| File.file?(path) && !File.symlink?(path) })
    end
  end

  describe "pattern matching on symlinks" do
    it "matches pattern on symlink names" do
      results = Fdr.search(paths: [@tmpdir], pattern: "link_to", max_depth: 1)

      assert results.size >= 2, "should find at least 2 symlinks matching 'link_to'"
      assert(results.any? { |path| path.include?("link_to_dir") })
      assert(results.any? { |path| path.include?("link_to_file") })
    end

    it "can filter symlinks by extension" do
      rb_link = File.join(@tmpdir, "link.rb")
      File.symlink(@real_file, rb_link)

      results = Fdr.search(paths: [@tmpdir], type: "l", extension: "rb", max_depth: 1)

      assert(results.any? { |path| path == rb_link })
    end
  end

  describe "no_ignore with follow (cross-option behavior)" do
    before do
      Dir.mkdir(File.join(@tmpdir, ".git"))

      files = create_gitignore_and_ignored_dir(@tmpdir)
      @ignored_dir = files[:ignored_dir]
      @ignored_file = files[:ignored_file]
      @link_to_ignored = files[:link_to_ignored]
    end

    it "finds files in ignored directories when both no_ignore and follow are true" do
      results = Fdr.search(
        paths: [@tmpdir],
        follow: true,
        no_ignore: true,
        type: "f"
      )

      assert_includes results, @ignored_file,
        "should find file in ignored directory when no_ignore: true"
    end

    it "follows symlinks to ignored directories with no_ignore" do
      results = Fdr.search(
        paths: [@tmpdir],
        follow: true,
        no_ignore: true,
        type: "f"
      )

      assert_includes results, File.join(@link_to_ignored, "file.txt"),
        "should traverse through symlink to ignored directory"
    end
  end
end
