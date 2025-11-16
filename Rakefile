# frozen_string_literal: true

require "bundler/gem_tasks"
require "fileutils"
require "minitest/test_task"
require "standard/rake"

task default: :check

desc "Compile native extension"
task :compile do
  Dir.chdir("ext/fdr_native") do
    ruby "extconf.rb"
    sh "make"

    FileUtils.mkdir_p "../../lib/fdr"
    ext = RUBY_PLATFORM.include?("darwin") ? "bundle" : "so"
    FileUtils.cp "fdr_native.#{ext}", "../../lib/fdr/fdr_native.#{ext}"
  end
end

Minitest::TestTask.create do |test|
  test.framework = %(require_relative "./spec/spec_helper.rb")
  test.libs = %w[lib spec .]
  test.test_globs = ["spec/**/*_spec.rb"]
  test.warning = false
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

namespace :rust do
  desc "Run Rust tests"
  task :test do
    Dir.chdir("ext/fdr_native") do
      sh "cargo test --all-targets --all-features"
    end
  end

  desc "Lint Rust code with clippy"
  task :lint do
    Dir.chdir("ext/fdr_native") do
      sh "cargo clippy --all-targets --all-features -- -D warnings"
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
task check: %i[rust:format rust:lint rust:test test standard]

desc "Build gem after compiling extension"
task build: :compile
