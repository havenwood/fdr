# frozen_string_literal: true

require_relative "lib/fdr/version"

Gem::Specification.new do |spec|
  spec.name = "fdr"
  spec.version = Fdr::VERSION
  spec.authors = ["Shannon Skipper"]
  spec.email = %w[shannonskipper@gmail.com]
  spec.description = "Fast `fd`-style file search and `rg`-style content search for Ruby."
  spec.summary = "Fast file and content search for Ruby"
  spec.homepage = "https://github.com/havenwood/fdr"
  spec.licenses = %w[MIT]
  spec.required_ruby_version = ">= 3.2"
  spec.requirements << "Rust 1.88 or newer"
  spec.files = (
    %w[
      LICENSE
      README.md
      ext/fdr_native/Cargo.lock
      ext/fdr_native/Cargo.toml
      ext/fdr_native/core/Cargo.toml
      ext/fdr_native/extconf.rb
      ext/fdr_native/ffi/Cargo.toml
      ext/fdr_native/ffi/build.rs
    ] +
    Dir["lib/**/*.rb"] +
    Dir["sig/**/*.rbs"] +
    Dir["ext/fdr_native/{core,ffi}/src/**/*.rs"]
  ).sort
  spec.require_paths = %w[lib]
  spec.extensions = ["ext/fdr_native/extconf.rb"]

  spec.add_dependency "rb_sys", "~> 0.9.130"
  spec.metadata["rubygems_mfa_required"] = "true"
  spec.metadata["source_code_uri"] = "https://github.com/havenwood/fdr"
  spec.metadata["bug_tracker_uri"] = "https://github.com/havenwood/fdr/issues"
end
