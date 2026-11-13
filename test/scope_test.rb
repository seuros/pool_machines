# frozen_string_literal: true

require 'minitest/autorun'
require 'pool_machines'

class ScopeTest < Minitest::Test
  def pool(scope)
    PoolMachines::Pool.with_scope(scope).tap { |p| p.register(0, 0) }
  end

  def test_a_handle_carries_its_pools_scope
    assert_equal 0, PoolMachines::Pool.new.scope
    p = pool(7)
    assert_equal [7, 0], [p.scope, p.issued]
    h = p.checkout(1)
    assert_equal [7, 0], [h.scope, h.slot]
    assert_equal 1, p.issued
  end

  def test_a_handle_from_another_scope_is_a_refusal_not_an_exception
    a = pool(1)
    b = pool(2)
    h = a.checkout(1)
    k = b.checkout(1)
    GC.start
    GC.compact
    assert_equal :stale, b.checkin(h, 2)
    assert_equal :stale, a.checkin(k, 2)
    assert_equal [1, 1], [a.checked_out, b.checked_out]
    assert_equal :idle, b.checkin(k, 2)
    assert_equal :idle, a.checkin(h, 2)
  end

  def test_a_handle_outlives_its_pool
    h = pool(3).checkout(1)
    GC.start
    assert_equal [3, 0], [h.scope, h.slot]
    assert_equal :stale, pool(4).checkin(h, 1)
  end

  def test_resumed_continues_the_sequence_and_refuses_earlier_handles
    p = pool(5)
    before = p.checkout(1)
    resumed = PoolMachines::Pool.resumed(5, p.issued)
    assert_equal [5, 1, 0], [resumed.scope, resumed.issued, resumed.checked_out]
    assert_nil resumed.checkout(0) # nothing is registered after a restart
    assert_equal :stale, resumed.checkin(before, 0)
    assert_equal :registered, resumed.register(0, 0)
    after = resumed.checkout(0)
    assert_equal 2, resumed.issued
    assert_equal :stale, resumed.checkin(before, 0) # same slot, another id
    assert_equal :idle, resumed.checkin(after, 0)
  end

  def test_a_spent_scope_is_exhausted
    p = PoolMachines::Pool.resumed(1, (2**48) - 2)
    p.register(0, 0)
    p.register(1, 0)
    assert_instance_of PoolMachines::Handle, p.checkout(1)
    assert_equal :exhausted, p.checkout(50)
    assert_equal :registered, p.register(2, 1) # the refusal did not move the clock
    assert_equal 1, p.checked_out
  end

  def test_scope_and_issued_validation
    [1.5, '1', nil].each do |bad|
      assert_raises(TypeError) { PoolMachines::Pool.with_scope(bad) }
      assert_raises(TypeError) { PoolMachines::Pool.resumed(bad, 0) }
      assert_raises(TypeError) { PoolMachines::Pool.resumed(0, bad) }
    end
    [-1, 65_536].each do |bad|
      assert_raises(RangeError) { PoolMachines::Pool.with_scope(bad) }
      assert_raises(RangeError) { PoolMachines::Pool.resumed(bad, 0) }
    end
    assert_raises(RangeError) { PoolMachines::Pool.resumed(0, 2**48) }
    assert_equal 65_535, PoolMachines::Pool.with_scope(65_535).scope
  end
end
