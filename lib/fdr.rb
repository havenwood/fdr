# frozen_string_literal: true

require "fdr/version"

begin
  require "fdr/fdr_native"
rescue LoadError => e
  raise LoadError, "Failed to load the fdr native extension on #{RUBY_PLATFORM} (#{e.message}). " \
    "Install Rust and reinstall fdr to build from source."
end

# Fast directory recursion for Ruby using Rust
module Fdr
  class << self
    def search(
      pattern: nil,
      paths: ["."],
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
      paths: ["."],
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

    alias_method :entries, :search
    alias_method :scan, :search

    private :native_search, :native_grep
  end
end
