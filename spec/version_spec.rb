# frozen_string_literal: true

require_relative 'spec_helper'

describe 'Fdr::VERSION' do
  it 'is defined' do
    assert defined?(Fdr::VERSION), 'Fdr::VERSION should be defined'
  end

  it 'is a String' do
    assert_kind_of String, Fdr::VERSION
  end

  it 'follows semantic versioning format' do
    assert_match(/\A\d+\.\d+\.\d+/, Fdr::VERSION)
  end

  it 'is not empty' do
    refute_empty Fdr::VERSION
  end

  it 'is frozen' do
    assert Fdr::VERSION.frozen?, 'VERSION should be frozen'
  end
end
