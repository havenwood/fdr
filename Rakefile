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
  require 'seen'

  raise "Expected Seen #{expected_version}, got #{Seen::VERSION}" unless Seen::VERSION == expected_version
  path_results = Seen.each_path(paths: [gem_home], extension: 'rb', no_ignore: true)
  raise 'Seen.each_path did not return an Enumerator' unless path_results.is_a?(Enumerator)
  raise 'Seen.each_path returned no Ruby files' if path_results.to_a.empty?
  raise 'Seen.each_path searched an empty path set' unless Seen.each_path(paths: []).to_a.empty?

  yielded = []
  Seen.each_path(paths: [gem_home], extension: 'rb', no_ignore: true) { |path| yielded << path }
  raise 'Seen.each_path ignored its block' if yielded.empty?

  line_results = Seen.each_line(
    pattern: 'MODULE SEEN',
    name: 'seen\.rb$',
    paths: [gem_home],
    extension: 'rb',
    no_ignore: true,
    content_case_sensitive: false
  )
  raise 'Seen.each_line did not return an Enumerator' unless line_results.is_a?(Enumerator)
  raise 'Seen.each_line returned no matches' if line_results.to_a.empty?
  raise 'Seen.each_line searched an empty path set' unless Seen.each_line(pattern: '.', paths: []).to_a.empty?

  binary = File.join(Dir.pwd, 'seen-smoke.bin')
  File.binwrite(binary, "a\0a\r\n")
  occurrences = Seen.each_line(pattern: 'a', paths: [binary], text: true, column: true).to_a
  expected = [[binary, 1, 1, "a\0a\r"], [binary, 1, 3, "a\0a\r"]]
  raise 'Seen.each_line text or column smoke failed' unless occurrences == expected
  match = Seen.each_line(pattern: '\\r', paths: [binary], text: true, byte_range: true).first
  raise 'Seen.each_line byte range smoke failed' unless match == [binary, 1, 3...4, "a\0a\r"]
  raise 'Seen.each_line byte range did not index text' unless match.last.byteslice(match[2]) == "\r"

  utf16 = File.join(Dir.pwd, 'seen-smoke-utf16.txt')
  File.binwrite(utf16, "\xFF\xFE".b + "needle wide\n".encode('UTF-16LE').b)
  expected = [[utf16, 1, 'needle wide']]
  raise 'Seen.each_line BOM detection smoke failed' unless Seen.each_line(pattern: 'needle', paths: [utf16]).to_a == expected

  old_api = %i[search grep entries scan].filter { |name| Seen.respond_to?(name) }
  raise "Seen exposed old API names: #{old_api.join(', ')}" unless old_api.empty?

  puts "Verified installed Seen #{Seen::VERSION}"
RUBY

task default: :check

desc "Compile the native extension"
task :compile do
  Dir.chdir("ext/seen_native") do
    ruby "extconf.rb"
    sh "make"
  end
  FileUtils.cp "ext/seen_native/seen_native.#{RbConfig::CONFIG["DLEXT"]}", "lib/seen/"
end

Minitest::TestTask.create do |test|
  test.framework = %(require_relative "./spec/spec_helper.rb")
  test.libs = %w[lib spec .]
  test.test_globs = ["spec/**/*_spec.rb"]
end
task test: :compile

desc "Clean build artifacts"
task :clean do
  FileUtils.rm_f Dir["lib/seen/*.{bundle,so}"]
  Dir.chdir("ext/seen_native") do
    sh "make clean" if File.exist?("Makefile")
  end
end

desc "Deep clean including Cargo artifacts"
task clobber: :clean do
  Dir.chdir("ext/seen_native") do
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
    Dir.chdir("ext/seen_native") do
      sh "cargo test --locked --all-targets --all-features"
    end
  end

  desc "Lint Rust code with clippy"
  task :lint do
    Dir.chdir("ext/seen_native") do
      sh "cargo clippy --locked --all-targets --all-features -- -D warnings"
    end
  end

  desc "Check Rust code formatting"
  task :format do
    Dir.chdir("ext/seen_native") do
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
  gemspec = Gem::Specification.load("seen.gemspec")
  gem_path = File.expand_path("pkg/#{gemspec.file_name}", __dir__)
  NativePackaging.verify_source!(gem_path)

  Dir.mktmpdir("seen-gem-verify") do |directory|
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
