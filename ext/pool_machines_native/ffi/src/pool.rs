use std::{cell::RefCell, sync::Arc};

use magnus::{Error, RModule, Ruby, function, method, prelude::*, typed_data::Obj};
use pool_machines::{Handle, Pool, Returned};

use crate::Count;

#[magnus::wrap(class = "PoolMachines::Handle", free_immediately, size)]
struct Lease {
    // Shared native identity survives pool GC; addresses cannot be recycled
    // while a handle exists. No Ruby values need marking.
    owner: Arc<()>,
    handle: Handle,
}

impl Lease {
    const fn slot(&self) -> usize {
        self.handle.slot()
    }
}

#[magnus::wrap(class = "PoolMachines::Pool", free_immediately, size)]
struct Wrapper {
    owner: Arc<()>,
    pool: RefCell<Pool<32>>,
}

impl Wrapper {
    fn new(ruby: &Ruby) -> Result<Self, Error> {
        let pool = Pool::new()
            .ok_or_else(|| Error::new(ruby.exception_runtime_error(), "invalid native capacity"))?;
        Ok(Self {
            owner: Arc::new(()),
            pool: RefCell::new(pool),
        })
    }

    fn register(ruby: &Ruby, this: Obj<Self>, slot: Count, now: Count) -> Result<bool, Error> {
        this.check_frozen()?;
        if slot.0 >= 32 {
            return Err(Error::new(
                ruby.exception_range_error(),
                "slot must be in 0...32",
            ));
        }
        let slot = usize::try_from(slot.0)
            .map_err(|_| Error::new(ruby.exception_range_error(), "slot overflow"))?;
        let now = now.duration();
        Ok(this.pool.borrow_mut().register(slot, now))
    }

    fn checkout(this: Obj<Self>, now: Count) -> Result<Option<Lease>, Error> {
        this.check_frozen()?;
        let handle = this.pool.borrow_mut().checkout(now.duration());
        Ok(handle.map(|handle| Lease {
            owner: Arc::clone(&this.owner),
            handle,
        }))
    }

    fn checkin(
        ruby: &Ruby,
        this: Obj<Self>,
        lease: &Lease,
        now: Count,
    ) -> Result<&'static str, Error> {
        this.check_frozen()?;
        if !Arc::ptr_eq(&this.owner, &lease.owner) {
            return Err(Error::new(
                ruby.exception_arg_error(),
                "handle belongs to another pool",
            ));
        }
        let returned = this.pool.borrow_mut().checkin(lease.handle, now.duration());
        match returned {
            Returned::Idle => Ok("idle"),
            Returned::Retire => Ok("retire"),
            _ => Err(Error::new(
                ruby.exception_arg_error(),
                "stale handle or backwards clock",
            )),
        }
    }

    fn begin_drain(this: Obj<Self>) -> Result<(), Error> {
        this.check_frozen()?;
        this.pool.borrow_mut().begin_drain();
        Ok(())
    }

    fn retire_idle(this: Obj<Self>, now: Count, idle_for: Count) -> Result<Option<usize>, Error> {
        this.check_frozen()?;
        let now = now.duration();
        let idle_for = idle_for.duration();
        let slot = this.pool.borrow_mut().retire_idle(now, idle_for);
        Ok(slot)
    }

    fn checked_out(&self) -> u32 {
        self.pool.borrow().checked_out()
    }
}

pub fn define(ruby: &Ruby, module: RModule) -> Result<(), Error> {
    let handle = module.define_class("Handle", ruby.class_object())?;
    handle.undef_default_alloc_func();
    handle.define_method("slot", method!(Lease::slot, 0))?;
    let class = module.define_class("Pool", ruby.class_object())?;
    class.define_singleton_method("new", function!(Wrapper::new, 0))?;
    class.define_method("register", method!(Wrapper::register, 2))?;
    class.define_method("checkout", method!(Wrapper::checkout, 1))?;
    class.define_method("checkin", method!(Wrapper::checkin, 2))?;
    class.define_method("begin_drain", method!(Wrapper::begin_drain, 0))?;
    class.define_method("retire_idle", method!(Wrapper::retire_idle, 2))?;
    class.define_method("checked_out", method!(Wrapper::checked_out, 0))
}
