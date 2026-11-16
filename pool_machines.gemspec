# frozen_string_literal: true
require_relative 'lib/pool_machines/version'
Gem::Specification.new do |spec|
  spec.name = 'pool_machines'
  spec.version = PoolMachines::VERSION
  spec.authors = ['Abdelkader Boudih']
  spec.email = ['oss@seuros.com']
  spec.summary = 'Generation-checked resource-slot checkout, recycling and draining for Ruby'
  spec.description = 'Keeps the roster of a resource pool: which slot is busy or idle, which idle slots are old ' \
                     'enough to retire, and whether a returned handle is stale. The caller owns the ' \
                     'resources and their lifecycle.'
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
    .reject { |p| p.end_with?('/tests.rs') || p.include?('/tests/') }
  spec.require_paths = ['lib']
  spec.extensions = ['ext/pool_machines_native/extconf.rb']
  spec.add_dependency 'rb_sys', '~> 0.9', '>= 0.9.130'
end
