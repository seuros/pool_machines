# frozen_string_literal: true
require 'minitest/autorun'
require 'pool_machines'

class PoolMachinesTest < Minitest::Test
  def test_vacant_checkout_reuse_and_stale_generation
    p = PoolMachines::Pool.new
    assert_nil p.checkout(0)
    assert p.register(0, 1)
    refute p.register(0, 1)
    h = p.checkout(2)
    assert_equal 0, h.slot
    assert_equal 1, p.checked_out
    assert_equal 'idle', p.checkin(h, 3)
    h2 = p.checkout(4)
    assert_raises(ArgumentError) { p.checkin(h, 100) }
    assert_equal 'idle', p.checkin(h2, 5) # rejected stale use must not advance time
    assert_raises(ArgumentError) { p.checkin(h2, 6) }
    assert_equal 0, p.checked_out
  end

  def test_instance_scope_and_gc
    a, b = PoolMachines::Pool.new, PoolMachines::Pool.new
    assert a.register(0, 0)
    assert b.register(0, 0)
    h, k = a.checkout(1), b.checkout(1)
    GC.start
    GC.compact
    assert_raises(ArgumentError) { b.checkin(h, 2) }
    assert_equal 1, b.checked_out
    assert_equal 'idle', b.checkin(k, 2)
    assert_equal 'idle', a.checkin(h, 2)
    assert_raises(TypeError) { PoolMachines::Handle.new }
    assert_raises(TypeError) { h.dup }
  end

  def test_drain_and_retirement
    p = PoolMachines::Pool.new
    assert p.register(0, 0)
    assert p.register(1, 0)
    h = p.checkout(1)
    assert_nil p.retire_idle(2, 3)
    p.begin_drain
    assert_nil p.checkout(3)
    refute p.register(2, 3)
    assert_equal 1, p.retire_idle(3, 100)
    assert_equal 'retire', p.checkin(h, 4)
    assert_nil p.retire_idle(5, 0)
    assert_equal 0, p.checked_out
  end

  def test_capacity_clock_and_validation
    p = PoolMachines::Pool.new
    32.times { |i| assert p.register(i, 0) }
    assert_raises(RangeError) { p.register(32, 0) }
    handles = 32.times.map { p.checkout(1) }
    assert_equal (0...32).to_a, handles.map(&:slot)
    assert_nil p.checkout(1)
    assert_raises(TypeError) { p.checkin(handles.first, 1.2) }
    assert_raises(RangeError) { p.checkin(handles.first, 2**64) }
    assert_equal 32, p.checked_out
    assert_equal 'idle', p.checkin(handles.first, 2)
    assert_nil p.checkout(1)
    [Float::NAN, Float::INFINITY, 1.5, '1'].each do |bad|
      assert_raises(TypeError) { p.retire_idle(2, bad) }
      assert_raises(TypeError) { p.register(bad, 2) }
    end
    assert_raises(RangeError) { p.checkout(-1) }
  end

  def test_frozen_mutation
    p = PoolMachines::Pool.new
    p.register(0, 0)
    h = p.checkout(1)
    p.freeze
    assert_raises(FrozenError) { p.register(1, 2) }
    assert_raises(FrozenError) { p.checkout(2) }
    assert_raises(FrozenError) { p.checkin(h, 2) }
    assert_raises(FrozenError) { p.begin_drain }
    assert_raises(FrozenError) { p.retire_idle(2, 0) }
    assert_equal 1, p.checked_out
  end
end
