# frozen_string_literal: true

require 'mkmf'
require 'rb_sys/mkmf'

if ENV['FD_DISABLE_NATIVE']
  warn 'FD_DISABLE_NATIVE is set'
  warn 'Skipping native extension'
  File.write('Makefile', "all:\n\t@echo 'Skipping'\ninstall:\n\t@echo 'Skipping'\n")
  exit
end

unless system('cargo --version > /dev/null 2>&1')
  warn 'WARNING: Cargo not found!'
  warn 'fdr requires Cargo to build the native extension'
  File.write('Makefile', "all:\n\t@echo 'Skipping'\ninstall:\n\t@echo 'Skipping'\n")
  abort
end

create_rust_makefile('fdr/fdr_native') do |r|
  r.ext_dir = 'ffi'
  r.profile = ENV.fetch('RB_SYS_CARGO_PROFILE', :release).to_sym
  r.extra_cargo_args << '--locked'
  r.extra_rustc_args.push('-C', "linker=#{ENV.fetch('CC')}") if ENV['CC']
  if RbConfig::CONFIG.fetch('host_os').include?('darwin')
    r.env['MACOSX_DEPLOYMENT_TARGET'] = ENV.fetch('MACOSX_DEPLOYMENT_TARGET', '11.0')
    # Rust before 1.98 can emit stripped Mach-O string tables that macOS 27 rejects.
    r.extra_rustc_args.push('-C', 'strip=none')
    r.extra_rustc_args.push('-C', 'link-arg=-Wl,-install_name,fdr_native.bundle')
  end
end

# rb_sys 0.9.128 clears the Mach-O install name after linking, which macOS 27 rejects.
makefile = File.read('Makefile')
makefile.sub!(/^\t\$\(Q\) .*install_name_tool -id "" \$\(DLLIB\)\n/, '')
File.write('Makefile', makefile)
