# frozen_string_literal: true

require_relative "lib/seen/version"

Gem::Specification.new do |spec|
  spec.name = "seen"
  spec.version = Seen::VERSION
  spec.authors = ["Shannon Skipper"]
  spec.email = %w[shannonskipper@gmail.com]
  spec.description = "Fast `fd`-style file search and `rg`-style content search for Ruby."
  spec.summary = "Fast file and content search for Ruby"
  spec.homepage = "https://github.com/havenwood/seen"
  spec.licenses = %w[MIT]
  spec.required_ruby_version = ">= 3.2"
  spec.requirements << "Rust 1.88 or newer"
  spec.files = (
    %w[
      LICENSE
      README.md
      ext/seen_native/Cargo.lock
      ext/seen_native/Cargo.toml
      ext/seen_native/core/Cargo.toml
      ext/seen_native/extconf.rb
      ext/seen_native/ffi/Cargo.toml
      ext/seen_native/ffi/build.rs
    ] +
    Dir["lib/**/*.rb"] +
    Dir["sig/**/*.rbs"] +
    Dir["ext/seen_native/{core,ffi}/src/**/*.rs"]
  ).sort
  spec.require_paths = %w[lib]
  spec.extensions = ["ext/seen_native/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9.130"
  spec.metadata["rubygems_mfa_required"] = "true"
  spec.metadata["source_code_uri"] = "https://github.com/havenwood/seen"
  spec.metadata["bug_tracker_uri"] = "https://github.com/havenwood/seen/issues"
end
