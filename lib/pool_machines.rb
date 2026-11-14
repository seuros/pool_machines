# frozen_string_literal: true
require_relative 'pool_machines/version'
require 'pool_machines_native/pool_machines_native'

module PoolMachines
  # Sealed: only the pool builds a Handle. No subclass, no `allocate`, and
  # the pool accepts nothing but an instance of exactly this class.
  Handle.singleton_class.undef_method(:allocate)
  Handle.define_singleton_method(:inherited) { |_| raise TypeError, "#{Handle} cannot be subclassed" }
end
