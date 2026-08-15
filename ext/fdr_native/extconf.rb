# frozen_string_literal: true

require "mkmf"
require "rb_sys/mkmf"
require "shellwords"

unless system("cargo", "--version", out: File::NULL, err: File::NULL)
  warn "WARNING: Cargo not found!"
  warn "fdr requires Cargo to build the native extension"
  abort
end

create_rust_makefile("fdr/fdr_native") do |r|
  r.ext_dir = "ffi"
  r.profile = ENV.fetch("RB_SYS_CARGO_PROFILE", :release).to_sym
  r.extra_cargo_args << "--locked"
  # rustc's linker takes a single program, so wrappers and flags in CC become
  # link args, as rb-sys does for RbConfig's CC.
  cc_words = Shellwords.split(ENV.fetch("CC", ""))
  cc_words.shift if cc_words.first&.end_with?("ccache", "cachepot")
  if (linker = cc_words.shift)
    r.extra_rustc_args.push("-C", "linker=#{linker}")
    cc_words.each { |word| r.extra_rustc_args.push("-C", "link-arg=#{word}") }
  end
  if RbConfig::CONFIG.fetch("host_os").include?("darwin")
    r.env["MACOSX_DEPLOYMENT_TARGET"] = ENV.fetch("MACOSX_DEPLOYMENT_TARGET", "11.0")
    # Rust before 1.98 can emit stripped Mach-O string tables that macOS 27 rejects.
    r.extra_rustc_args.push("-C", "strip=none")
  end
end
