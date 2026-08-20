# frozen_string_literal: true

require 'rubygems/package'

# Validates source and precompiled gem contents.
module NativePackaging
  module_function

  def verify(gem_pattern)
    abort 'Pass the precompiled gem path: rake native:verify[path]' unless gem_pattern

    matches = Dir[File.expand_path(gem_pattern)]
    abort "Expected one precompiled gem, found #{matches.length}: #{gem_pattern}" unless matches.one?

    verify_gem!(matches.fetch(0))
  end

  def verify_gem!(gem_path)
    package = Gem::Package.new(gem_path)
    spec = package.spec

    assert_native_metadata!(spec)
    assert_native_files!(spec)
    assert_matching_contents!(package, spec)
    puts "Verified precompiled gem #{File.basename(gem_path)}"
  end

  def verify_source!(gem_path)
    package = Gem::Package.new(gem_path)
    spec = package.spec

    assert_source_metadata!(spec)
    assert_source_files!(spec)
    assert_matching_contents!(package, spec)
    puts "Verified source gem #{File.basename(gem_path)}"
  end

  def assert_matching_contents!(package, spec)
    return if package.contents.sort == spec.files.sort

    raise 'Gem archive and specification contain different files'
  end

  def assert_native_metadata!(spec)
    assert_platform!(spec.platform.to_s)
    assert_no_source_build!(spec)
    assert_native_ruby_upper_bound!(spec.required_ruby_version)
    assert_rubygems_range!(spec.required_rubygems_version) if spec.platform.to_s.include?('-linux-')
  end

  def assert_source_metadata!(spec)
    raise "Unexpected source platform: #{spec.platform}" unless spec.platform.to_s == 'ruby'
    raise "Unexpected source extensions: #{spec.extensions.inspect}" unless spec.extensions == [SOURCE_EXTENSION]
    raise 'Source gem does not require Rust 1.88' unless spec.requirements.include?('Rust 1.88 or newer')

    assert_source_dependency!(spec)
  end

  def assert_source_dependency!(spec)
    dependency = spec.dependencies.find { |candidate| candidate.name == 'rb_sys' }
    return if dependency&.requirement == RB_SYS_REQUIREMENT

    raise 'Source gem does not require rb_sys ~> 0.9.128'
  end

  def assert_source_files!(spec)
    raise 'Source gem does not contain Cargo.lock' unless spec.files.include?('ext/fdr_native/Cargo.lock')
    raise 'Source gem contains a precompiled extension' if spec.files.any? { |file| file.match?(EXTENSION_PATTERN) }

    generated = spec.files.grep(%r{\Aext/fdr_native/target/})
    raise "Source gem contains generated Cargo files: #{generated.join(', ')}" unless generated.empty?
  end

  def assert_platform!(platform)
    raise "Unexpected native platform: #{platform}" unless BUILD_PLATFORMS.include?(platform)
  end

  def assert_no_source_build!(spec)
    raise 'Native gem still declares an extension build' unless spec.extensions.empty?
    raise 'Native gem still depends on rb_sys' if spec.dependencies.any? { |dependency| dependency.name == 'rb_sys' }
    raise 'Native gem still requires Rust' if spec.requirements.any? { |requirement| requirement.start_with?('Rust ') }
  end

  def assert_native_files!(spec)
    extension = spec.platform.os == 'darwin' ? 'bundle' : 'so'
    expected = RUBY_VERSIONS.map { |version| "lib/fdr/#{version}/fdr_native.#{extension}" }
    actual = spec.files.grep(EXTENSION_PATTERN)

    raise "Unexpected native extensions: #{actual.inspect}" unless actual.sort == expected.sort
    raise 'Native gem contains extension sources' if spec.files.any? { |file| file.start_with?('ext/') }
  end

  def assert_native_ruby_upper_bound!(requirement)
    latest = Gem::Version.new(RUBY_VERSIONS.last).segments
    next_ruby = Gem::Version.new("#{latest.fetch(0)}.#{latest.fetch(1) + 1}")
    raise 'Native gem accepts Ruby without a bundled ABI' if requirement.satisfied_by?(next_ruby)
  end

  def assert_rubygems_range!(requirement)
    minimum = Gem::Version.new('3.3.22')
    raise 'Linux native gem does not require RubyGems 3.3.22' unless requirement.satisfied_by?(minimum)
    raise 'Linux native gem accepts RubyGems older than 3.3.22' if requirement.satisfied_by?(Gem::Version.new('3.3.21'))
  end
end
