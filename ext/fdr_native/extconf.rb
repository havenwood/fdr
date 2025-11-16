# frozen_string_literal: true

require "mkmf"
require "rb_sys/mkmf"

unless system("cargo", "--version", out: File::NULL, err: File::NULL)
  warn "WARNING: Cargo not found!"
  warn "fdr requires Cargo to build the native extension"
  abort
end

create_rust_makefile("fdr/fdr_native") do |r|
  r.ext_dir = "ffi"
  r.profile = ENV.fetch("RB_SYS_CARGO_PROFILE", :release).to_sym
end
