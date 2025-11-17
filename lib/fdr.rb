# frozen_string_literal: true

require 'fdr/version'
require 'fdr/fdr_native'

# Fast directory recursion for Ruby using Rust
module Fdr
  class << self
    def search(
      pattern: nil,
      paths: ['.'],
      hidden: false,
      no_ignore: false,
      case_sensitive: false,
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
  end
end
