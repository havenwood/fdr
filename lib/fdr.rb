# frozen_string_literal: true

require "fdr/version"

begin
  require "fdr/fdr_native"
rescue LoadError => e
  raise LoadError, "Failed to load the fdr native extension on #{RUBY_PLATFORM} (#{e.message}). " \
    "Install Rust and reinstall fdr to build from source."
end

# File and content search for Ruby
module Fdr
  # Tracks the implicit root separately from an explicit ".", like `fd` and `rg`.
  CWD = ["."].freeze
  private_constant :CWD

  class << self
    def search(
      pattern: nil,
      paths: CWD,
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
      changed_before: nil,
      ignore_error: true,
      ignore_file: [],
      &
    )
      results = native_search(
        pattern:,
        paths:,
        strip_cwd_prefix: paths.equal?(CWD),
        ignore_error:,
        ignore_file:,
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
      results.each(&)
      results
    end

    def grep(
      pattern:,
      name: nil,
      paths: CWD,
      hidden: false,
      no_ignore: false,
      case_sensitive: false,
      content_case_sensitive: true,
      text: false,
      column: false,
      byte_range: false,
      glob: false,
      full_path: false,
      follow: false,
      max_depth: nil,
      min_depth: nil,
      extension: nil,
      exclude: [],
      min_size: nil,
      max_size: nil,
      changed_within: nil,
      changed_before: nil,
      ignore_error: true,
      ignore_file: [],
      &
    )
      results = native_grep(
        pattern:,
        name:,
        paths:,
        strip_cwd_prefix: paths.equal?(CWD),
        ignore_error:,
        ignore_file:,
        hidden:,
        no_ignore:,
        case_sensitive:,
        content_case_sensitive:,
        text:,
        column:,
        byte_range:,
        glob:,
        full_path:,
        follow:,
        max_depth:,
        min_depth:,
        extension:,
        exclude:,
        min_size:,
        max_size:,
        changed_within:,
        changed_before:
      )
      results.each(&)
      results
    end

    private :native_search, :native_grep
  end
end
