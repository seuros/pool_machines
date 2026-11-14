use std::cell::RefCell;

use magnus::{Error, RModule, Ruby, Value, function, method, prelude::*, typed_data::Obj};
use matryona_machines::ruby::{Class, Registry, Symbols, duration, exact_typed, integer, keep};
use pool_machines::{DrainBegin, Handle, Refusal, Registration, Returned};

const CAPACITY: usize = 32;

type Core = pool_machines::Pool<CAPACITY>;

matryona_machines::symbols! {
    /// The core's refusals as Symbols: the snake case of each variant.
    Refusals for Refusal as "a refusal" {
        Draining => "draining",
        TimeRegression => "time_regression",
        UnknownSlot => "unknown_slot",
        Busy => "busy",
        Exhausted => "exhausted",
        Stale => "stale",
    }
}

matryona_machines::symbols! {
    /// The core's honoured `register` answers as Symbols.
    Registrations for Registration as "a registration" {
        Registered => "registered",
        AlreadyIdle => "already_idle",
    }
}

matryona_machines::symbols! {
    /// The core's honoured `checkin` answers as Symbols.
    Returns for Returned as "a return" {
        Idle => "idle",
        Retire => "retire",
    }
}

matryona_machines::symbols! {
    /// The core's `begin_drain` answers as Symbols.
    DrainBegins for DrainBegin as "a drain" {
        Begun => "begun",
        AlreadyDraining => "already_draining",
    }
}

fn refused(ruby: &Ruby, refused: Refusal) -> Value {
    Refusals::symbol(ruby, refused).as_value()
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

/// `PoolMachines::Handle`, the class defined at init and pinned there: a
/// constant reassigned later neither receives a handle nor passes for one.
struct Classes {
    handle: Class,
}

static CLASSES: Registry<Classes> = Registry::new("pool_machines");

/// `value` when it is exactly a `PoolMachines::Handle`: `TypeError` for
/// anything else, a subclass included. Only the binding builds one, from a
/// handle the core issued, so its contents need no further check.
fn issued(ruby: &Ruby, value: Value) -> Result<Handle, Error> {
    let class = CLASSES.get(ruby)?.handle.get(ruby);
    let handle: Obj<Issued> =
        exact_typed(ruby, value, class, "handle must be a PoolMachines::Handle")?;
    Ok(handle.0)
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
        let scope = integer(scope, "scope")?;
        Self::build(ruby, Core::with_scope(scope), "invalid native capacity")
    }

    fn resumed(ruby: &Ruby, scope: Value, issued: Value) -> Result<Self, Error> {
        let (scope, issued) = (integer(scope, "scope")?, integer(issued, "issued")?);
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
        let (slot, now) = (integer(slot, "slot")?, duration(now, "now")?);
        let outcome = this.core.borrow_mut().register(slot, now);
        Ok(match outcome {
            Ok(registration) => Registrations::symbol(ruby, registration).as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn checkout(ruby: &Ruby, this: Obj<Self>, now: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let now = duration(now, "now")?;
        // Look the class up before the core issues a handle: a failure here
        // must not leave a checkout behind that Ruby never received.
        let class = CLASSES.get(ruby)?.handle.get(ruby);
        // Clone-then-keep: the core decides on a copy, kept only once the
        // answer exists and the pool is still unfrozen and unchanged.
        keep(
            ruby,
            this,
            &this.core,
            |core| core.checkout(now),
            |checkout| {
                Ok(match checkout {
                    Ok(Some(handle)) => ruby.obj_wrap_as(Issued(handle), class).as_value(),
                    Ok(None) => ruby.qnil().as_value(),
                    Err(refusal) => refused(ruby, refusal),
                })
            },
        )
    }

    fn checkin(ruby: &Ruby, this: Obj<Self>, handle: Value, now: Value) -> Result<Value, Error> {
        this.check_frozen()?;
        let handle = issued(ruby, handle)?;
        let now = duration(now, "now")?;
        let outcome = this.core.borrow_mut().checkin(handle, now);
        Ok(match outcome {
            Ok(returned) => Returns::symbol(ruby, returned).as_value(),
            Err(refusal) => refused(ruby, refusal),
        })
    }

    fn begin_drain(ruby: &Ruby, this: Obj<Self>) -> Result<Value, Error> {
        this.check_frozen()?;
        let outcome = this.core.borrow_mut().begin_drain();
        Ok(DrainBegins::symbol(ruby, outcome).as_value())
    }

    fn retire_idle(
        ruby: &Ruby,
        this: Obj<Self>,
        now: Value,
        idle_for: Value,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let (now, idle_for) = (duration(now, "now")?, duration(idle_for, "idle_for")?);
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
    CLASSES.set(
        ruby,
        Classes {
            handle: Class::pin(ruby, handle),
        },
    )?;
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
