# frozen_string_literal: true
require_relative 'lib/pool_machines/version'
Gem::Specification.new do |spec|
  spec.name = 'pool_machines'
  spec.version = PoolMachines::VERSION
  spec.authors = ['Abdelkader Boudih']
  spec.email = ['oss@seuros.com']
  spec.summary = 'Native generation-checked resource-slot policy for Ruby'
  spec.description = 'Initial native roster model with draining and retirement. ' \
                     'Caller owns resources and execution. Local sibling Rust checkout required.'
  spec.homepage = 'https://github.com/seuros/pool_machines'
  spec.license = 'MIT'
  spec.required_ruby_version = '>= 4.0.0'
  spec.metadata['source_code_uri'] = spec.homepage
  spec.metadata['rubygems_mfa_required'] = 'true'
  spec.metadata['cargo_crate_name'] = 'pool_machines_native'
  spec.metadata['cargo_manifest_path'] = 'ext/pool_machines_native/ffi/Cargo.toml'
  spec.files = Dir.glob(%w[
    lib/**/*.rb sig/**/*.rbs ext/**/*.{rs,rb,toml,md} Cargo.toml
    Cargo.lock README.md CHANGELOG.md LICENSE.txt
  ]).select { |p| File.file?(p) }
  spec.require_paths = ['lib']
  spec.extensions = ['ext/pool_machines_native/extconf.rb']
  spec.add_dependency 'rb_sys', '~> 0.9', '>= 0.9.130'
end
