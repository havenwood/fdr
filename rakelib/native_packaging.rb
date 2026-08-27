# frozen_string_literal: true

# Validates the source gem's contents and metadata.
module NativePackaging
  SOURCE_EXTENSION = "ext/seen_native/extconf.rb"
  RB_SYS_REQUIREMENT = Gem::Requirement.new("~> 0.9.130")
  RUST_REQUIREMENT = "Rust 1.88 or newer"
end

require_relative "native_packaging/verification"
