# frozen_string_literal: true

require 'minitest/autorun'
require 'pool_machines'

class PoolMachinesTest < Minitest::Test
  def test_vacant_checkout_reuse_and_single_use_handles
    p = PoolMachines::Pool.new
    assert_nil p.checkout(0)
    assert_equal :registered, p.register(0, 1)
    assert_equal :already_idle, p.register(0, 1)
    h = p.checkout(2)
    assert_instance_of PoolMachines::Handle, h
    assert_equal 0, h.slot
    assert_equal 1, p.checked_out
    assert_equal :busy, p.register(0, 2)
    assert_equal :idle, p.checkin(h, 3)
    h2 = p.checkout(4)
    assert_equal :stale, p.checkin(h, 100)
    assert_equal :idle, p.checkin(h2, 5) # a refused checkin must not advance time
    assert_equal :stale, p.checkin(h2, 6)
    assert_equal 0, p.checked_out
  end

  def test_drain_and_retirement
    p = PoolMachines::Pool.new
    assert_equal :registered, p.register(0, 0)
    assert_equal :registered, p.register(1, 0)
    h = p.checkout(1)
    assert_nil p.retire_idle(2, 3)
    refute_predicate p, :draining?
    assert_equal :begun, p.begin_drain
    assert_equal :already_draining, p.begin_drain
    assert_predicate p, :draining?
    assert_equal :draining, p.checkout(3)
    assert_equal :draining, p.register(2, 3)
    assert_equal 1, p.retire_idle(3, 100)
    assert_equal :retire, p.checkin(h, 4)
    assert_nil p.retire_idle(5, 0)
    assert_equal 0, p.checked_out
  end

  def test_capacity_and_unknown_slots
    p = PoolMachines::Pool.new
    assert_equal 32, p.capacity
    assert_equal 32, PoolMachines::Pool::CAPACITY
    32.times { |i| assert_equal :registered, p.register(i, 0) }
    assert_equal :unknown_slot, p.register(32, 0)
    assert_equal :unknown_slot, p.register((2**64) - 1, 0)
    handles = 32.times.map { p.checkout(1) }
    assert_equal (0...32).to_a, handles.map(&:slot)
    assert_nil p.checkout(1)
    assert_equal 32, p.checked_out
    assert_equal :idle, p.checkin(handles.first, 2)
    assert_equal 31, p.checked_out
  end

  def test_refusals_leave_the_clock_alone_and_drain_answers_record_it
    p = PoolMachines::Pool.new
    assert_equal :registered, p.register(0, 5)
    assert_equal :time_regression, p.register(1, 4)
    assert_equal :time_regression, p.checkout(4)
    assert_equal :time_regression, p.retire_idle(4, 0)
    assert_equal :unknown_slot, p.register(99, 1_000)
    assert_equal :already_idle, p.register(0, 1_000)
    assert_equal :registered, p.register(1, 5) # none of the above moved the clock
    h = p.checkout(6)
    assert_equal :time_regression, p.checkin(h, 5)
    assert_equal :idle, p.checkin(h, 6)
    assert_nil p.retire_idle(10, 60) # honoured: nothing is old enough at 10
    assert_equal :time_regression, p.register(2, 9)

    empty = PoolMachines::Pool.new
    assert_nil empty.checkout(10) # honoured: nothing is idle at 10
    assert_equal :time_regression, empty.register(0, 9)
  end

  def test_malformed_input_raises
    p = PoolMachines::Pool.new
    p.register(0, 0)
    h = p.checkout(1)
    [Float::NAN, Float::INFINITY, 1.5, '1', nil].each do |bad|
      assert_raises(TypeError) { p.retire_idle(2, bad) }
      assert_raises(TypeError) { p.retire_idle(bad, 2) }
      assert_raises(TypeError) { p.register(bad, 2) }
      assert_raises(TypeError) { p.register(1, bad) }
      assert_raises(TypeError) { p.checkout(bad) }
      assert_raises(TypeError) { p.checkin(h, bad) }
    end
    assert_raises(TypeError) { p.checkin(1, 2) }
    assert_raises(RangeError) { p.checkin(h, 2**64) }
    assert_raises(RangeError) { p.checkout(-1) }
    assert_raises(RangeError) { p.register(-1, 0) }
    assert_raises(TypeError) { PoolMachines::Handle.new }
    assert_raises(TypeError) { PoolMachines::Pool.allocate }
    assert_raises(TypeError) { h.dup }
    assert_equal :idle, p.checkin(h, 2)
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
    assert_equal 1, p.issued
    refute_predicate p, :draining?
  end
end
