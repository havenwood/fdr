# frozen_string_literal: true

require "mkmf"
require "rb_sys/mkmf"

abort "fdr does not support Windows" if Gem.win_platform?

cargo = ENV.fetch("CARGO", "cargo")
unless system(cargo, "--version", out: File::NULL, err: File::NULL)
  abort "fdr requires Cargo to build the native extension"
end

create_rust_makefile("fdr/fdr_native") do |r|
  r.ext_dir = "ffi"
  r.extra_cargo_args << "--locked"
end
