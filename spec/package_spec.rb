# frozen_string_literal: true

require_relative 'spec_helper'

class PackageSpec < Minitest::Test
  def setup
    @gemspec = Gem::Specification.load(File.expand_path('../fdr.gemspec', __dir__))
  end

  def test_includes_cargo_lock
    assert_includes @gemspec.files, 'ext/fdr_native/Cargo.lock'
  end

  def test_excludes_generated_cargo_files
    assert_empty @gemspec.files.grep(%r{\Aext/fdr_native/target/})
  end

  def test_every_packaged_file_exists
    missing_files = @gemspec.files.reject { |file| File.file?(File.expand_path("../#{file}", __dir__)) }

    assert_empty missing_files
  end
end
