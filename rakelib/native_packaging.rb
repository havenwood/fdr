# frozen_string_literal: true

require "rb_sys/extensiontask"

# Builds and validates the source gem's native extension.
module NativePackaging
  SOURCE_EXTENSION = "ext/fdr_native/extconf.rb"
  RB_SYS_REQUIREMENT = Gem::Requirement.new("~> 0.9.130")
  RUST_REQUIREMENT = "Rust 1.88 or newer"

  # Resolves Cargo metadata from Fdr's nested extension workspace.
  class ExtensionTask < RbSys::ExtensionTask
    def cargo_metadata
      @cargo_metadata ||= Dir.chdir(File.expand_path("../ext/fdr_native", __dir__)) do
        RbSys::Cargo::Metadata.new_or_inferred(name)
      end
    end

    def extconf
      File.join(cargo_metadata.workspace_root, "extconf.rb")
    end
  end

  module_function

  def configure(gemspec)
    ExtensionTask.new("fdr-native", gemspec) do |extension|
      extension.lib_dir = "lib/fdr"
    end
  end
end

require_relative "native_packaging/verification"
