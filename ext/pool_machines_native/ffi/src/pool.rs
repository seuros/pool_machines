use std::cell::RefCell;

use magnus::{Error, RModule, Ruby, Value, function, method, prelude::*, typed_data::Obj};
use pool_machines::{DrainBegin, Handle, Refusal, Registration, Returned};

use crate::{Count, Scope};

const CAPACITY: usize = 32;

type Core = pool_machines::Pool<CAPACITY>;

/// Ruby symbol name of a refusal: the snake case of the core variant.
const fn refusal(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Draining => "draining",
        Refusal::TimeRegression => "time_regression",
        Refusal::UnknownSlot => "unknown_slot",
        Refusal::Busy => "busy",
        Refusal::Exhausted => "exhausted",
        Refusal::Stale => "stale",
    }
}

fn refused(ruby: &Ruby, refused: Refusal) -> Value {
    ruby.to_symbol(refusal(refused)).as_value()
}

/// `PoolMachines::Handle`: the core's `Handle`. It carries the scope of the
/// pool that issued it; the core refuses it anywhere else.
#[magnus::wrap(class = "PoolMachines::Handle", free_immediately, size)]
struct Issued(Handle);

impl Issued {
    const fn slot(&self) -> usize {
        self.0.slot()
    }

    const fn scope(&self) -> u16 {
        self.0.scope()
    }
}

#[magnus::wrap(class = "PoolMachines::Pool", free_immediately, size)]
struct Pool {
    core: RefCell<Core>,
}

impl Pool {
    fn build(ruby: &Ruby, core: Option<Core>, why: &'static str) -> Result<Self, Error> {
        core.map(|core| Self {
            core: RefCell::new(core),
        })
        .ok_or_else(|| Error::new(ruby.exception_range_error(), why))
    }

    fn new(ruby: &Ruby) -> Result<Self, Error> {
        Self::build(ruby, Core::new(), "invalid native capacity")
    }

    fn with_scope(ruby: &Ruby, scope: Scope) -> Result<Self, Error> {
        Self::build(ruby, Core::with_scope(scope.0), "invalid native capacity")
    }

    fn resumed(ruby: &Ruby, scope: Scope, issued: Count) -> Result<Self, Error> {
        Self::build(
            ruby,
            Core::resumed(scope.0, issued.0),
            "issued is past the last sequence of a scope (2**48 - 1)",
        )
    }

    fn scope(&self) -> u16 {
        self.core.borrow().scope()
    }

    fn issued(&self) -> u64 {
        self.core.borrow().issued()
    }

    fn capacity(&self) -> usize {
        self.core.borrow().capacity()
    }

    fn checked_out(&self) -> u32 {
        self.core.borrow().checked_out()
    }

    fn is_draining(&self) -> bool {
        self.core.borrow().is_draining()
    }

    fn register(ruby: &Ruby, this: Obj<Self>, slot: Count, now: Count) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().register(slot.slot(), now.duration());
        Ok(match outcome {
            Ok(Registration::Registered) => ruby.to_symbol("registered").as_value(),
            Ok(Registration::AlreadyIdle) => ruby.to_symbol("already_idle").as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn checkout(ruby: &Ruby, this: Obj<Self>, now: Count) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().checkout(now.duration());
        Ok(match outcome {
            Ok(Some(handle)) => ruby.obj_wrap(Issued(handle)).as_value(),
            Ok(None) => ruby.qnil().as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn checkin(
        ruby: &Ruby,
        this: Obj<Self>,
        handle: Obj<Issued>,
        now: Count,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().checkin(handle.0, now.duration());
        Ok(match outcome {
            Ok(Returned::Idle) => ruby.to_symbol("idle").as_value(),
            Ok(Returned::Retire) => ruby.to_symbol("retire").as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn begin_drain(ruby: &Ruby, this: Obj<Self>) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().begin_drain();
        Ok(ruby
            .to_symbol(match outcome {
                DrainBegin::Begun => "begun",
                DrainBegin::AlreadyDraining => "already_draining",
            })
            .as_value())
    }

    fn retire_idle(
        ruby: &Ruby,
        this: Obj<Self>,
        now: Count,
        idle_for: Count,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this
            .core
            .borrow_mut()
            .retire_idle(now.duration(), idle_for.duration());
        Ok(match outcome {
            Ok(Some(slot)) => ruby.integer_from_u64(slot as u64).as_value(),
            Ok(None) => ruby.qnil().as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }
}

pub fn define(ruby: &Ruby, module: RModule) -> Result<(), Error> {
    let handle = module.define_class("Handle", ruby.class_object())?;
    handle.undef_default_alloc_func();
    handle.define_method("slot", method!(Issued::slot, 0))?;
    handle.define_method("scope", method!(Issued::scope, 0))?;
    let class = module.define_class("Pool", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.const_set("CAPACITY", CAPACITY)?;
    class.define_singleton_method("new", function!(Pool::new, 0))?;
    class.define_singleton_method("with_scope", function!(Pool::with_scope, 1))?;
    class.define_singleton_method("resumed", function!(Pool::resumed, 2))?;
    class.define_method("scope", method!(Pool::scope, 0))?;
    class.define_method("issued", method!(Pool::issued, 0))?;
    class.define_method("capacity", method!(Pool::capacity, 0))?;
    class.define_method("checked_out", method!(Pool::checked_out, 0))?;
    class.define_method("draining?", method!(Pool::is_draining, 0))?;
    class.define_method("register", method!(Pool::register, 2))?;
    class.define_method("checkout", method!(Pool::checkout, 1))?;
    class.define_method("checkin", method!(Pool::checkin, 2))?;
    class.define_method("begin_drain", method!(Pool::begin_drain, 0))?;
    class.define_method("retire_idle", method!(Pool::retire_idle, 2))
}
