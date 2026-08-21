# frozen_string_literal: true

require "bundler/gem_tasks"
require "fileutils"
require "minitest/test_task"
require "tmpdir"
require_relative "rakelib/native_packaging"

require "standard/rake"

GEM_SMOKE_TEST = <<~'RUBY'
  gem_home, expected_version = ARGV
  Gem.use_paths(gem_home)
  require 'fdr'

  raise "Expected Fdr #{expected_version}, got #{Fdr::VERSION}" unless Fdr::VERSION == expected_version
  raise 'Fdr.search returned no Ruby files' if Fdr.search(paths: [gem_home], extension: 'rb', no_ignore: true).empty?
  raise 'Fdr.search searched an empty path set' unless Fdr.search(paths: []).empty?

  grep_results = Fdr.grep(
    pattern: 'MODULE FDR',
    name: 'fdr\.rb$',
    paths: [gem_home],
    extension: 'rb',
    no_ignore: true,
    content_case_sensitive: false
  )
  raise 'Fdr.grep returned no matches' if grep_results.empty?
  raise 'Fdr.grep searched an empty path set' unless Fdr.grep(pattern: '.', paths: []).empty?
  raise 'Fdr exposed removed search aliases' if Fdr.respond_to?(:entries) || Fdr.respond_to?(:scan)

  puts "Verified installed Fdr #{Fdr::VERSION}"
RUBY

task default: :check

desc "Compile the native extension"
task :compile do
  Dir.chdir("ext/fdr_native") do
    ruby "extconf.rb"
    sh "make"
  end
  FileUtils.cp "ext/fdr_native/fdr_native.#{RbConfig::CONFIG["DLEXT"]}", "lib/fdr/"
end

Minitest::TestTask.create do |test|
  test.framework = %(require_relative "./spec/spec_helper.rb")
  test.libs = %w[lib spec .]
  test.test_globs = ["spec/**/*_spec.rb"]
end
task test: :compile

desc "Clean build artifacts"
task :clean do
  FileUtils.rm_f Dir["lib/fdr/*.{bundle,so}"]
  Dir.chdir("ext/fdr_native") do
    sh "make clean" if File.exist?("Makefile")
  end
end

desc "Deep clean including Cargo artifacts"
task clobber: :clean do
  Dir.chdir("ext/fdr_native") do
    sh "cargo clean"
  end
end

desc "Validate RBS signatures"
task :rbs do
  sh "rbs -I sig validate"
end

namespace :rust do
  desc "Run Rust tests"
  task :test do
    Dir.chdir("ext/fdr_native") do
      sh "cargo test --locked --all-targets --all-features"
    end
  end

  desc "Lint Rust code with clippy"
  task :lint do
    Dir.chdir("ext/fdr_native") do
      sh "cargo clippy --locked --all-targets --all-features -- -D warnings"
    end
  end

  desc "Check Rust code formatting"
  task :format do
    Dir.chdir("ext/fdr_native") do
      sh "cargo fmt --all --check"
    end
  end

  desc "Run all Rust checks"
  task check: %i[format lint test]
end

desc "Run all checks (Rust + Ruby)"
task check: %i[rust:format rust:lint rust:test test rbs standard]

desc "Build, install, and smoke-test the packaged gem"
task "gem:verify" => :build do
  gemspec = Gem::Specification.load("fdr.gemspec")
  gem_path = File.expand_path("pkg/#{gemspec.file_name}", __dir__)
  NativePackaging.verify_source!(gem_path)

  Dir.mktmpdir("fdr-gem-verify") do |directory|
    gem_home = File.join(directory, "gem-home")

    Bundler.with_unbundled_env do
      sh Gem.ruby, "-S", "gem", "install", gem_path, "--install-dir", gem_home, "--no-document"

      Dir.chdir(directory) do
        sh Gem.ruby, "-e", GEM_SMOKE_TEST, gem_home, gemspec.version.to_s
      end
    end
  end
end

desc "Verify the packaged gem before pushing the release source"
task "release:source_control_push" => %i[check gem:verify]
