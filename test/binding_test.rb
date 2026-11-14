# frozen_string_literal: true

require 'minitest/autorun'
require 'pool_machines'

# The native boundary: a sealed Handle, accepted only as exactly that class,
# and a checkout whose answer is built without running Ruby code.
class BindingTest < Minitest::Test
  Handle = PoolMachines::Handle

  def pool
    PoolMachines::Pool.new.tap { |p| p.register(0, 0) }
  end

  def state(pool)
    [pool.checked_out, pool.issued, pool.draining?]
  end

  # `class ForgedHandle < Handle; end` raises TypeError, yet Ruby binds the
  # constant before `inherited` runs: the subclass leaks.
  def forged_class
    return Object.const_get(:ForgedHandle) if Object.const_defined?(:ForgedHandle)

    eval('class ForgedHandle < PoolMachines::Handle; end', TOPLEVEL_BINDING, __FILE__, __LINE__)
    flunk 'a named subclass of Handle was accepted'
  rescue TypeError
    Object.const_get(:ForgedHandle)
  end

  def test_handle_is_sealed
    assert_raises(TypeError) { Class.new(Handle) }
    assert_raises(NoMethodError) { Handle.allocate }
    assert_raises(TypeError) { Handle.new }
    # The named form raises too, but the constant leaks: the native boundary,
    # not the seal, is the authority.
    assert_operator forged_class, :<, Handle
  end

  def test_a_handle_cannot_be_forged_or_copied
    assert_raises(NoMethodError) { forged_class.allocate }
    assert_raises(TypeError) { forged_class.new }
    h = pool.checkout(1)
    assert_raises(TypeError) { h.dup }
    assert_raises(TypeError) { h.clone }
    assert_raises(TypeError) { Marshal.dump(h) }
  end

  def test_only_exactly_a_handle_is_accepted
    p = pool
    h = p.checkout(1)
    [Object.new, forged_class, Data.define(:slot).new(0), h.slot].each do |fake|
      assert_raises(TypeError) { p.checkin(fake, 2) }
    end
    assert_equal [1, 1, false], state(p)
    # A singleton class on the real handle does not change what it is, and a
    # redefined `==` on a forged class fools nothing: classes compare by pointer.
    def h.note = :mine
    forged_class.define_singleton_method(:==) { |_| true }
    assert_equal :idle, p.checkin(h, 2)
  ensure
    forged_class.singleton_class.remove_method(:==) if forged_class.singleton_class.method_defined?(:==, false)
  end

  # The probe: with the constant swapped for String, a checkout wrapped the
  # core's handle in a String. The class is resolved once, at init.
  def test_the_handle_class_is_resolved_once_at_init
    p = pool
    PoolMachines.send(:remove_const, :Handle)
    PoolMachines.const_set(:Handle, String)
    GC.start
    h = p.checkout(1)
  ensure
    PoolMachines.send(:remove_const, :Handle)
    PoolMachines.const_set(:Handle, Handle)
    assert_instance_of Handle, h
    assert_equal :idle, p.checkin(h, 2)
  end

  # The answer is wrapped natively: no `new`, `allocate` or `initialize` runs,
  # so none can raise, re-enter or freeze the pool between decision and store.
  def test_building_the_answer_runs_no_ruby_code
    p = pool
    calls = []
    Handle.define_singleton_method(:new) { |*| calls << :new && raise(IOError) }
    Handle.define_method(:initialize) { |*| calls << :initialize && p.freeze }
    h = p.checkout(1)
    assert_instance_of Handle, h
    assert_empty calls
    refute_predicate p, :frozen?
    assert_equal [1, 1, false], state(p)
  ensure
    Handle.singleton_class.remove_method(:new)
    verbose, $VERBOSE = $VERBOSE, nil # Ruby warns about removing any `initialize`
    Handle.remove_method(:initialize)
    $VERBOSE = verbose
  end
end
