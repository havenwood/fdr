# frozen_string_literal: true

require "fileutils"
require "open3"
require "tmpdir"
require_relative "spec_helper"

class NativeLoaderSpec < Minitest::Test
  def test_loads_the_native_extension
    extension = compiled_extension
    skip "Native extension is not compiled" unless extension

    Dir.mktmpdir("seen-native-loader") do |directory|
      lib = build_library(directory, extension)
      stderr, status = load_seen(lib)

      assert status.success?, stderr
    end
  end

  def test_reports_a_missing_native_extension
    Dir.mktmpdir("seen-native-loader") do |directory|
      lib = build_library(directory, nil)
      stderr, status = load_seen(lib)

      refute status.success?, "loading without any native extension should fail"
      assert_match(/Failed to load the seen native extension/, stderr)
      assert_match(/Install Rust/, stderr)
    end
  end

  private

  def compiled_extension
    Dir[File.expand_path("../lib/seen/seen_native.{bundle,so}", __dir__)].first
  end

  def build_library(directory, extension)
    lib = File.join(directory, "lib")
    seen = File.join(lib, "seen")
    FileUtils.mkdir_p(seen)
    FileUtils.cp(File.expand_path("../lib/seen.rb", __dir__), lib)
    FileUtils.cp(File.expand_path("../lib/seen/version.rb", __dir__), seen)
    FileUtils.cp(extension, File.join(seen, File.basename(extension))) if extension
    lib
  end

  def load_seen(lib)
    script = "require 'seen'; results = Seen.each_path(paths: [#{lib.inspect}]); abort unless results.is_a?(Enumerator) && results.any?"
    _, stderr, status = Open3.capture3(
      {"RUBYOPT" => nil, "RUBYLIB" => nil}, Gem.ruby, "--disable-gems", "-I#{lib}", "-e", script
    )
    [stderr, status]
  end
end
