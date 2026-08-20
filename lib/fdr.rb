# frozen_string_literal: true

require 'rbconfig'
require 'fdr/version'

ruby_api_version = RUBY_VERSION[/\A\d+\.\d+/]
versioned_extension = File.expand_path("fdr/#{ruby_api_version}/fdr_native", __dir__)
extension_suffixes = [RbConfig::CONFIG['DLEXT'], RbConfig::CONFIG['DLEXT2']].compact.uniq
versioned_extension = extension_suffixes
                      .map { |suffix| "#{versioned_extension}.#{suffix}" }
                      .find { |path| File.file?(path) }

require versioned_extension || 'fdr/fdr_native'

# Fast directory recursion for Ruby using Rust
module Fdr
  class << self
    def search(
      pattern: nil,
      paths: ['.'],
      hidden: false,
      no_ignore: false,
      case_sensitive: false,
      glob: false,
      full_path: false,
      follow: false,
      max_depth: nil,
      min_depth: nil,
      type: nil,
      extension: nil,
      exclude: [],
      min_size: nil,
      max_size: nil,
      changed_within: nil,
      changed_before: nil
    )
      native_search(
        pattern:,
        paths:,
        hidden:,
        no_ignore:,
        case_sensitive:,
        glob:,
        full_path:,
        follow:,
        max_depth:,
        min_depth:,
        type:,
        extension:,
        exclude:,
        min_size:,
        max_size:,
        changed_within:,
        changed_before:
      )
    end

    def grep(
      pattern:,
      name: nil,
      paths: ['.'],
      hidden: false,
      no_ignore: false,
      case_sensitive: true,
      glob: false,
      full_path: false,
      follow: false,
      max_depth: nil,
      min_depth: nil,
      type: nil,
      extension: nil,
      exclude: [],
      min_size: nil,
      max_size: nil,
      changed_within: nil,
      changed_before: nil
    )
      native_grep(
        pattern:,
        name:,
        paths:,
        hidden:,
        no_ignore:,
        case_sensitive:,
        glob:,
        full_path:,
        follow:,
        max_depth:,
        min_depth:,
        type:,
        extension:,
        exclude:,
        min_size:,
        max_size:,
        changed_within:,
        changed_before:
      )
    end

    alias entries search
    alias scan search

    private :native_search, :native_grep
  end
end
