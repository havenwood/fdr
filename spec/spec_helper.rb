# frozen_string_literal: true

lib = File.expand_path('../lib', __dir__)
$LOAD_PATH.prepend(lib) unless $LOAD_PATH.include?(lib)

require 'fdr'
require 'minitest/autorun'
require 'minitest/hell'
require 'minitest/pride'

result = Fdr.search(extension: 'rb', paths: ['lib'], max_depth: 1)
abort 'Native extension produced wrong result' unless result.is_a?(Array) && !result.empty?
