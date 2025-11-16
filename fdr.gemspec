# frozen_string_literal: true

lib = File.expand_path('lib', __dir__)
$LOAD_PATH.prepend(lib) unless $LOAD_PATH.include?(lib)
require 'fdr/version'

Gem::Specification.new do |spec|
  spec.name = 'fdr'
  spec.version = Fdr::VERSION
  spec.authors = ['Shannon Skipper']
  spec.email = %w[shannonskipper@gmail.com]
  spec.description = 'Fast fd-like file search from Ruby'
  spec.summary = 'Fdr is a fast fd-inspired file search library for Ruby with Rust native extensions.'
  spec.homepage = 'https://github.com/havenwood/fdr'
  spec.licenses = %w[MIT]
  spec.required_ruby_version = '>= 3.2'
  spec.files = %w[Gemfile LICENSE Rakefile README.md] +
               Dir['lib/**/*.rb'] +
               Dir['ext/**/*.{rb,rs,toml}']
  spec.require_paths = %w[lib]
  spec.extensions = ['ext/fdr_native/extconf.rb']

  spec.add_dependency 'rb_sys', '~> 0.9'
  spec.metadata['rubygems_mfa_required'] = 'true'
  spec.metadata['source_code_uri'] = 'https://github.com/havenwood/fdr'
  spec.metadata['bug_tracker_uri'] = 'https://github.com/havenwood/fdr/issues'
end
