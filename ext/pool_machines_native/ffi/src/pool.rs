use std::cell::RefCell;
use std::sync::OnceLock;

use magnus::{
    Error, RClass, RModule, Ruby, TryConvert, Value, function, method, prelude::*,
    rb_sys::AsRawValue, typed_data::Obj, value::Opaque,
};
use pool_machines::{DrainBegin, Handle, Refusal, Registration, Returned};

use crate::{count, nanos};

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

/// `PoolMachines::Handle`, the class defined at init. Resolved once: a
/// constant reassigned later neither receives a handle nor passes for one.
static HANDLE: OnceLock<Opaque<RClass>> = OnceLock::new();

fn handle_class(ruby: &Ruby) -> Result<RClass, Error> {
    HANDLE
        .get()
        .map(|class| ruby.get_inner(*class))
        .ok_or_else(|| {
            Error::new(
                ruby.exception_runtime_error(),
                "the pool_machines extension is not initialised",
            )
        })
}

/// Whether `value` is an instance of exactly `class` (`instance_of?`), by
/// pointer, so no `==` the caller could redefine runs. A singleton class on
/// the instance itself does not change what it is.
fn instance_of(value: Obj<Issued>, class: RClass) -> bool {
    let actual = value.class();
    if actual.as_raw() == class.as_raw() {
        return true;
    }
    let singleton = value
        .singleton_class()
        .is_ok_and(|own| own.as_raw() == actual.as_raw());
    singleton
        && actual
            .superclass()
            .is_ok_and(|parent| parent.as_raw() == class.as_raw())
}

/// `value` when it is exactly a `PoolMachines::Handle`: `TypeError` for
/// anything else, a subclass included. Only the binding builds one, from a
/// handle the core issued, so its contents need no further check.
fn issued(ruby: &Ruby, value: Value) -> Result<Handle, Error> {
    let class = handle_class(ruby)?;
    match Obj::<Issued>::try_convert(value) {
        Ok(handle) if instance_of(handle, class) => Ok(handle.0),
        _ => Err(Error::new(
            ruby.exception_type_error(),
            format!(
                "handle must be a PoolMachines::Handle, not {}",
                value.class()
            ),
        )),
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

    fn with_scope(ruby: &Ruby, scope: Value) -> Result<Self, Error> {
        let scope = crate::scope(scope)?;
        Self::build(ruby, Core::with_scope(scope), "invalid native capacity")
    }

    fn resumed(ruby: &Ruby, scope: Value, issued: Value) -> Result<Self, Error> {
        let (scope, issued) = (crate::scope(scope)?, count(issued, "issued")?);
        Self::build(
            ruby,
            Core::resumed(scope, issued),
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

    fn register(ruby: &Ruby, this: Obj<Self>, slot: Value, now: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let (slot, now) = (crate::slot(slot)?, nanos(now, "now")?);
        let outcome = this.core.borrow_mut().register(slot, now);
        Ok(match outcome {
            Ok(Registration::Registered) => ruby.to_symbol("registered").as_value(),
            Ok(Registration::AlreadyIdle) => ruby.to_symbol("already_idle").as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn checkout(ruby: &Ruby, this: Obj<Self>, now: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let now = nanos(now, "now")?;
        // Look the class up before the core issues a handle: a failure here
        // must not leave a checkout behind that Ruby never received.
        let class = handle_class(ruby)?;
        // Clone-then-keep: the core decides on a copy, kept only once the
        // answer exists and the pool is still unfrozen and unchanged.
        let before = this.core.borrow().clone();
        let mut after = before.clone();
        let answer = match after.checkout(now) {
            Ok(Some(handle)) => ruby.obj_wrap_as(Issued(handle), class).as_value(),
            Ok(None) => ruby.qnil().as_value(),
            Err(refusal) => return Ok(refused(ruby, refusal)),
        };
        this.check_frozen()?;
        let mut held = this.core.borrow_mut();
        if *held != before {
            return Err(Error::new(
                ruby.exception_runtime_error(),
                "the pool changed while the answer was built; the checkout was not applied",
            ));
        }
        *held = after;
        drop(held);
        Ok(answer)
    }

    fn checkin(ruby: &Ruby, this: Obj<Self>, handle: Value, now: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let handle = issued(ruby, handle)?;
        let now = nanos(now, "now")?;
        let outcome = this.core.borrow_mut().checkin(handle, now);
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
        now: Value,
        idle_for: Value,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let (now, idle_for) = (nanos(now, "now")?, nanos(idle_for, "idle_for")?);
        let outcome = this.core.borrow_mut().retire_idle(now, idle_for);
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
    ruby.gc_register_mark_object(handle);
    HANDLE.set(handle.into()).map_err(|_| {
        Error::new(
            ruby.exception_runtime_error(),
            "the pool_machines extension was initialised twice",
        )
    })?;
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
