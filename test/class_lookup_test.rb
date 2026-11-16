# frozen_string_literal: true

require 'minitest/autorun'
require 'open3'
require 'rbconfig'

# The classes of the values the extension wraps (Pool, Handle) are fixed
# when it loads. Nothing a caller does later, redefining a constant or
# `Object.const_get`, reaches a value being built. Each case runs in a fresh
# process: the class is fixed once per process, at load.
class ClassLookupTest < Minitest::Test
  LIB = File.expand_path('../lib', __dir__)

  PRELUDE = <<~RUBY
    require 'pool_machines'
    P = PoolMachines
  RUBY

  # Runs `body` after PRELUDE in a new Ruby; answers its last line of output.
  def isolated(body)
    out, err, status = Open3.capture3(RbConfig.ruby, '-I', LIB, '-e', PRELUDE + body)
    assert status.success?, "exited #{status.exitstatus}: #{err}"
    out.lines.last&.chomp
  end

  # Bug: a Pool constant redefined before the first pool made Pool.new answer
  # an instance of the new class, which has none of the pool's methods.
  def test_a_redefined_pool_constant_does_not_change_the_pool_class
    assert_equal 'true registered 0', isolated(<<~RUBY)
      original = P::Pool
      P.send(:remove_const, :Pool)
      P.const_set(:Pool, Class.new)
      pool = original.new
      puts [pool.instance_of?(original), pool.register(0, 0), pool.checked_out].join(' ')
    RUBY
  end

  # A Handle checked out after its constant was redefined is still a Handle
  # the pool takes back.
  def test_a_redefined_handle_constant_does_not_change_the_handle_class
    assert_equal 'true idle 0', isolated(<<~RUBY)
      original = P::Handle
      pool = P::Pool.new
      pool.register(0, 0)
      P.send(:remove_const, :Handle)
      P.const_set(:Handle, Class.new)
      handle = pool.checkout(1)
      puts [handle.instance_of?(original), pool.checkin(handle, 2), pool.checked_out].join(' ')
    RUBY
  end

  # Bug: building the first pool ran `Object.const_get`, a caller's code in
  # the middle of a native call. No Ruby runs while a value is built now.
  def test_building_a_value_runs_no_const_get
    assert_equal '0 false Pool Handle', isolated(<<~RUBY)
      hits = 0
      Object.define_singleton_method(:const_get) do |*args|
        hits += 1
        super(*args)
      end
      pool = P::Pool.with_scope(3)
      pool.register(0, 0)
      handle = pool.checkout(1)
      puts [hits, pool.frozen?, pool.class.name.split('::').last,
            handle.class.name.split('::').last].join(' ')
    RUBY
  end

  # Bug: a redefined `Object.const_get` that raised hit an `unwrap` in the
  # extension: an uncatchable fatal, and the process exited.
  def test_a_raising_const_get_does_not_kill_the_process
    assert_equal 'registered idle', isolated(<<~RUBY)
      Object.define_singleton_method(:const_get) { |*| raise IOError, 'lookup ran mid-call' }
      pool = P::Pool.resumed(0, 0)
      registered = pool.register(0, 0)
      handle = pool.checkout(1)
      puts [registered, pool.checkin(handle, 2)].join(' ')
    RUBY
  end
end
