# frozen_string_literal: true

lib = File.expand_path("../lib", __dir__)
$LOAD_PATH.prepend(lib) unless $LOAD_PATH.include?(lib)

require "seen"
require "minitest/autorun"
require "minitest/hell"
require "minitest/pride"

Minitest::Test.prove_it!

module Results
  def path_results(**options)
    Seen.each_path(**options).to_a.sort
  end

  def line_results(**options)
    Seen.each_line(**options).each_with_object({}) do |(path, line_number, text), results|
      (results[path] ||= {})[line_number] = text
    end
  end
end

Minitest::Test.include(Results)

result = Seen.each_path(extension: "rb", paths: ["lib"], max_depth: 1)
abort "Native extension produced wrong result" unless result.is_a?(Enumerator) && result.any?
