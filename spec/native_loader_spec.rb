# frozen_string_literal: true

require 'fileutils'
require 'open3'
require 'tmpdir'
require_relative 'spec_helper'

class NativeLoaderSpec < Minitest::Test
  def test_loads_a_versioned_native_extension
    extension = compiled_extension
    skip 'Native extension is not compiled' unless extension

    Dir.mktmpdir('fdr-native-loader') do |directory|
      lib = build_versioned_library(directory, extension)
      stderr, status = load_fdr(lib)

      assert status.success?, stderr
    end
  end

  def test_reports_a_missing_native_extension
    Dir.mktmpdir('fdr-native-loader') do |directory|
      lib = build_versioned_library(directory, nil)
      stderr, status = load_fdr(lib)

      refute status.success?, 'loading without any native extension should fail'
      assert_match(/native extension for Ruby #{Regexp.escape(RUBY_VERSION[/\A\d+\.\d+/])}/, stderr)
      assert_match(/Install Rust/, stderr)
    end
  end

  private

  def compiled_extension
    Dir[File.expand_path('../lib/fdr/fdr_native.{bundle,so}', __dir__)].first
  end

  def build_versioned_library(directory, extension)
    lib = File.join(directory, 'lib')
    fdr = File.join(lib, 'fdr')
    versioned = File.join(fdr, RUBY_VERSION[/\A\d+\.\d+/])
    FileUtils.mkdir_p(versioned)
    FileUtils.cp(File.expand_path('../lib/fdr.rb', __dir__), lib)
    FileUtils.cp(File.expand_path('../lib/fdr/version.rb', __dir__), fdr)
    FileUtils.cp(extension, File.join(versioned, File.basename(extension))) if extension
    lib
  end

  def load_fdr(lib)
    script = "require 'fdr'; abort unless Fdr.search(paths: [#{lib.inspect}]).is_a?(Array)"
    _, stderr, status = Open3.capture3(
      {'RUBYOPT' => nil, 'RUBYLIB' => nil}, Gem.ruby, '--disable-gems', "-I#{lib}", '-e', script
    )
    [stderr, status]
  end
end
