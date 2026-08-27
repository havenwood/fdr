# frozen_string_literal: true

require "mkmf"
require "rb_sys/mkmf"

abort "seen does not support Windows" if Gem.win_platform?

cargo = ENV.fetch("CARGO", "cargo")
unless system(cargo, "--version", out: File::NULL, err: File::NULL)
  abort "seen requires Cargo to build the native extension"
end

create_rust_makefile("seen/seen_native") do |r|
  r.ext_dir = "ffi"
  r.extra_cargo_args << "--locked"
end
