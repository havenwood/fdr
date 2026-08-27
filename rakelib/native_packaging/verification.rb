# frozen_string_literal: true

require "rubygems/package"

# Validates source gem contents.
module NativePackaging
  module_function

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

    raise "Gem archive and specification contain different files"
  end

  def assert_source_metadata!(spec)
    raise "Unexpected source platform: #{spec.platform}" unless spec.platform.to_s == "ruby"
    raise "Unexpected source extensions: #{spec.extensions.inspect}" unless spec.extensions == [SOURCE_EXTENSION]
    raise "Source gem does not require #{RUST_REQUIREMENT}" unless spec.requirements.include?(RUST_REQUIREMENT)

    assert_source_dependency!(spec)
  end

  def assert_source_dependency!(spec)
    dependency = spec.dependencies.find { |candidate| candidate.name == "rb_sys" }
    return if dependency&.requirement == RB_SYS_REQUIREMENT

    raise "Source gem does not require rb_sys #{RB_SYS_REQUIREMENT}"
  end

  def assert_source_files!(spec)
    raise "Source gem does not contain Cargo.lock" unless spec.files.include?("ext/seen_native/Cargo.lock")
    raise "Source gem does not contain the native build script" unless spec.files.include?("ext/seen_native/ffi/build.rs")
    raise "Source gem contains a compiled extension" if spec.files.any? { |file| file.match?(/\.(?:bundle|so)\z/) }

    generated = spec.files.grep(%r{\Aext/seen_native/(?:target|tmp)/})
    raise "Source gem contains generated Cargo files: #{generated.join(", ")}" unless generated.empty?
  end
end
