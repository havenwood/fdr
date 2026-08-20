# frozen_string_literal: true

require 'fileutils'
require 'rb_sys/extensiontask'
require 'rb_sys/version'

# Builds and validates platform gems containing versioned native extensions.
module NativePackaging
  RUBY_VERSIONS = %w[3.2 3.3 3.4 4.0].freeze
  BUILD_PLATFORMS = %w[
    arm64-darwin
    x86_64-darwin
    aarch64-linux-gnu
    x86_64-linux-gnu
    aarch64-linux-musl
    x86_64-linux-musl
  ].freeze
  DOCK_PLATFORMS = {
    'aarch64-linux-gnu' => 'aarch64-linux',
    'x86_64-linux-gnu' => 'x86_64-linux'
  }.freeze
  SOURCE_EXTENSION = 'ext/fdr_native/extconf.rb'
  RB_SYS_REQUIREMENT = Gem::Requirement.new('~> 0.9.128')
  EXTENSION_PATTERN = %r{\Alib/fdr/(?:\d+\.\d+/)?fdr_native\.(?:bundle|so)\z}

  # Resolves Cargo metadata from Fdr's nested extension workspace.
  class ExtensionTask < RbSys::ExtensionTask
    def cargo_metadata
      @cargo_metadata ||= Dir.chdir(File.expand_path('../ext/fdr_native', __dir__)) do
        RbSys::Cargo::Metadata.new_or_inferred(name)
      end
    end

    def extconf
      File.join(cargo_metadata.workspace_root, 'extconf.rb')
    end
  end

  module_function

  def configure(gemspec)
    ExtensionTask.new('fdr-native', gemspec) do |extension|
      extension.lib_dir = 'lib/fdr'
      extension.cross_compiling do |spec|
        spec.requirements.reject! { |requirement| requirement.start_with?('Rust ') }
      end
    end
  end

  def build(platform)
    validate_platform!(platform)
    FileUtils.mkdir_p(File.join(cargo_home, 'registry'))
    Rake::FileUtilsExt.sh dock_environment(platform), *dock_command(platform)
  end

  def validate_platform!(platform)
    return if BUILD_PLATFORMS.include?(platform)

    abort "Platform must be one of: #{BUILD_PLATFORMS.join(', ')}"
  end

  def cargo_home
    ENV.fetch('CARGO_HOME') { File.join(Dir.home, '.cargo') }
  end

  def dock_environment(platform)
    dock_platform = DOCK_PLATFORMS.fetch(platform, platform)
    environment = {
      'CARGO_HOME' => cargo_home,
      'RCD_IMAGE' => "rbsys/#{dock_platform}:#{RbSys::VERSION}"
    }
    return environment unless RbConfig::CONFIG.fetch('host_os').include?('darwin')

    FileUtils.mkdir_p('tmp')
    environment.merge('TMPDIR' => File.expand_path('tmp'))
  end

  def dock_command(platform)
    [
      'bundle', 'exec', 'rb-sys-dock', '--platform', platform,
      '--ruby-versions', RUBY_VERSIONS.join(','), '--', container_command
    ]
  end

  def container_command
    environment = 'BUNDLE_APP_CONFIG=/tmp/fdr-bundle-config BUNDLE_WITHOUT=development BUNDLE_JOBS=1'
    [
      "#{environment} bundle install",
      "#{environment} bundle exec rake native:$RUBY_TARGET gem"
    ].join(' && ')
  end
end

require_relative 'native_packaging/verification'
