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
end
