# frozen_string_literal: true

require_relative "spec_helper"
require "rake"

class PackageSpec < Minitest::Test
  def setup
    @gemspec = Gem::Specification.load(File.expand_path("../fdr.gemspec", __dir__))
  end

  def test_includes_cargo_lock
    assert_includes @gemspec.files, "ext/fdr_native/Cargo.lock"
  end

  def test_includes_rbs_signature
    assert_includes @gemspec.files, "sig/fdr.rbs"
  end

  def test_excludes_generated_cargo_files
    assert_empty @gemspec.files.grep(%r{\Aext/fdr_native/(?:target|tmp)/})
  end

  def test_excludes_rust_tests
    assert_empty @gemspec.files.grep(%r{\Aext/fdr_native/core/tests/})
  end

  def test_every_packaged_file_exists
    missing_files = @gemspec.files.reject { |file| File.file?(File.expand_path("../#{file}", __dir__)) }

    assert_empty missing_files
  end

  def test_release_runs_the_full_check_and_packaged_gem_verification
    previous_application = Rake.application
    Rake.application = Rake::Application.new
    load File.expand_path("../Rakefile", __dir__)

    prerequisites = Rake::Task["release:source_control_push"].prerequisites
    assert_includes prerequisites, "check"
    assert_includes prerequisites, "gem:verify"
  ensure
    Rake.application = previous_application
  end
end
