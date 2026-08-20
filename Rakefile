# frozen_string_literal: true

require 'bundler/gem_tasks'
require 'fileutils'
require 'tmpdir'
require_relative 'rakelib/native_packaging'

begin
  require 'rubocop/rake_task'
rescue LoadError
  # Cross-builds install packaging dependencies only.
end

GEMSPEC = Gem::Specification.load('fdr.gemspec') || abort('Could not load fdr.gemspec')
NativePackaging.configure(GEMSPEC)

task default: :check

desc 'Run Ruby tests'
task test: :compile do
  Dir.glob('spec/**/*_spec.rb').each do |file|
    ruby '-Ilib:spec', file
  end
end

desc 'Clean build artifacts'
task :clean do
  FileUtils.rm_f Dir['lib/fdr/*.{bundle,so}']
  Dir.chdir('ext/fdr_native') do
    sh 'make clean' if File.exist?('Makefile')
  end
end

desc 'Deep clean including Cargo artifacts'
task clobber: :clean do
  Dir.chdir('ext/fdr_native') do
    sh 'cargo clean'
  end
end

RuboCop::RakeTask.new if defined?(RuboCop::RakeTask)

namespace :rust do
  desc 'Run Rust tests'
  task :test do
    Dir.chdir('ext/fdr_native') do
      sh 'cargo test --locked --all-targets --all-features'
    end
  end

  desc 'Lint Rust code with clippy'
  task :lint do
    Dir.chdir('ext/fdr_native') do
      sh 'cargo clippy --locked --all-targets --all-features -- -D warnings'
    end
  end

  desc 'Check Rust code formatting'
  task :format do
    Dir.chdir('ext/fdr_native') do
      sh 'cargo fmt --all --check'
    end
  end

  desc 'Run all Rust checks'
  task check: %i[format lint test]
end

namespace :native do
  desc 'Build a precompiled gem for a supported platform'
  task :build, [:platform] do |_task, arguments|
    NativePackaging.build(arguments[:platform])
  end

  desc 'Verify precompiled gem metadata and contents'
  task :verify, [:gem_path] do |_task, arguments|
    NativePackaging.verify(arguments[:gem_path])
  end
end

desc 'Run all checks (Rust + Ruby)'
task check: %i[rust:format rust:lint rust:test test rubocop]

desc 'Build gem after compiling extension'
task build: :compile

desc 'Build, install, and smoke-test the packaged gem'
task 'gem:verify' => :build do
  gemspec = Gem::Specification.load('fdr.gemspec')
  gem_path = File.expand_path("pkg/#{gemspec.file_name}", __dir__)
  NativePackaging.verify_source!(gem_path)

  Dir.mktmpdir('fdr-gem-verify') do |directory|
    gem_home = File.join(directory, 'gem-home')
    smoke_test = <<~'RUBY'
      gem_home, expected_version = ARGV
      Gem.use_paths(gem_home)
      require 'fdr'

      raise "Expected Fdr #{expected_version}, got #{Fdr::VERSION}" unless Fdr::VERSION == expected_version
      raise 'Fdr.search returned no Ruby files' if Fdr.search(paths: [gem_home], extension: 'rb').empty?

      puts "Verified installed Fdr #{Fdr::VERSION}"
    RUBY

    Bundler.with_unbundled_env do
      sh Gem.ruby, '-S', 'gem', 'install', gem_path, '--install-dir', gem_home, '--no-document'

      Dir.chdir(directory) do
        sh Gem.ruby, '-e', smoke_test, gem_home, gemspec.version.to_s
      end
    end
  end
end

desc 'Verify the packaged gem before pushing the release source'
task 'release:source_control_push' => 'gem:verify'
