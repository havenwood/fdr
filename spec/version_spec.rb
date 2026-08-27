# frozen_string_literal: true

require_relative "spec_helper"

describe "Seen::VERSION" do
  it "is defined" do
    assert defined?(Seen::VERSION), "Seen::VERSION should be defined"
  end

  it "is a String" do
    assert_kind_of String, Seen::VERSION
  end

  it "follows semantic versioning format" do
    assert_match(/\A\d+\.\d+\.\d+/, Seen::VERSION)
  end

  it "is not empty" do
    refute_empty Seen::VERSION
  end

  it "is frozen" do
    assert Seen::VERSION.frozen?, "VERSION should be frozen"
  end
end
