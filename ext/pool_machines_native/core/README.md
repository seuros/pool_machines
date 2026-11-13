# pool-machines

> **AMS Little**, NCC-4214, *Little*-class service tender.

Engine-room flight manual: track resource checkouts, returns, and retirement.

Allocation-free `no_std` lifecycle decisions for fixed resource slots.
Register externally created resources, check out the slot that has been idle
longest, and return it with the `Handle` the checkout issued. A handle is good
for one `checkin`; a spent, foreign or forged one is refused as `Stale`.
Begin draining to refuse new checkouts; returned resources then retire rather
than becoming idle. Idle retirement frees a slot for external destruction.

The adapter must serialize these decisions with resource storage and actually
destroy retired resources. There is no automatic construction, health check,
async waiting, timeout driver, forced reclamation of busy resources, or
runtime ownership guarantee.

```rust
use core::time::Duration;
use pool_machines::{DrainBegin, Pool, Refusal, Registration, Returned};

let at = Duration::from_secs;
let mut pool = Pool::<2>::with_scope(7).expect("two slots");
assert_eq!(pool.register(0, at(1)), Ok(Registration::Registered));
assert_eq!(pool.register(0, at(1)), Ok(Registration::AlreadyIdle));
assert_eq!(pool.register(2, at(1)), Err(Refusal::UnknownSlot));

let handle = pool.checkout(at(2)).expect("honoured").expect("slot 0 is idle");
assert_eq!((handle.slot(), handle.scope()), (0, 7));
assert_eq!(pool.checkout(at(2)), Ok(None));
assert_eq!(pool.checkin(handle, at(1)), Err(Refusal::TimeRegression));
assert_eq!(pool.checkin(handle, at(3)), Ok(Returned::Idle));
assert_eq!(pool.checkin(handle, at(3)), Err(Refusal::Stale));

assert_eq!(pool.retire_idle(at(4), at(60)), Ok(None));
assert_eq!(pool.begin_drain(), DrainBegin::Begun);
assert_eq!(pool.checkout(at(4)), Err(Refusal::Draining));
assert_eq!(pool.retire_idle(at(4), at(60)), Ok(Some(0)));

// Another pool, another scope: it refuses a handle it did not issue.
let mut other = Pool::<2>::with_scope(8).expect("two slots");
assert_eq!(other.checkin(handle, at(0)), Err(Refusal::Stale));
```

## Contract

- **Capacity.** `Pool<N>` holds `N` slots, `1..=u32::MAX`; the constructors
  answer `None` otherwise. Each slot is vacant, idle since an instant, or busy
  under one handle. `capacity()` is `N`; `checked_out()` counts busy slots.
- **Refusals.** `Refusal` is the crate's one refusal type and a refused call
  changes nothing: not a slot, not a checkout credit, not an issued id, not
  the observed clock. `register` answers `Draining`, `TimeRegression`,
  `UnknownSlot` or `Busy`, checked in that order; `checkout` answers
  `Draining`, `TimeRegression` or `Exhausted`; `checkin` answers `Stale` or
  `TimeRegression`; `retire_idle` answers `TimeRegression`. `begin_drain`
  never refuses.
- **Drains.** `checkout` and `retire_idle` return `Result<Option<_>, Refusal>`.
  `Ok(None)` is an honoured answer, "nothing is idle at `now`" and "nothing is
  old enough at `now`", not a refusal.
- **Time.** Instants and spans are `core::time::Duration` since an epoch the
  caller picks. The pool keeps the latest instant an honoured call showed it
  and refuses an earlier one as `TimeRegression`. Time is checked without
  being recorded: only an honoured call moves the clock. The two drain
  answers are honoured calls about `now`, so they record it, `Ok(None)`
  included; a refusal never does.
- **Repeats.** `register` on a slot that already holds an idle resource
  answers `Registration::AlreadyIdle`, and `begin_drain` on a draining pool
  answers `DrainBegin::AlreadyDraining`: the effect asked for holds, the pool
  can see it, and nothing changes (not the instant the slot went idle, not
  the clock). A repeated `checkin` is **not** honoured: the pool keeps nothing
  about a checkout once it is returned, so the repeat is `Stale` like any
  other handle that names no live checkout.
- **Ids.** A `Handle` carries its slot and an id from one
  `periscope_machines::Issuer`: the pool's scope in the top 16 bits, a
  sequence counted from one below. An id is never reissued, so a spent handle
  stays `Stale` after its slot is checked out again. A checkout refused for
  any reason consumes no id. After `periscope_machines::SEQUENCE_MAX`
  (2^48 - 1) checkouts `checkout` answers `Exhausted`.
- **Scopes.** A handle is only meaningful to the pool that issued it.
  `Pool::with_scope(scope)` chooses the scope; `new()` is scope zero.
  `Pool::scope` and `Handle::scope` read it. A handle of another scope, or one
  the pool never issued, is `Stale` before any slot is read. Give each pool
  its own scope: the core reads no clock and no random source, so it cannot
  pick one, and two pools left in the same scope issue the same handles.
- **Restarts.** A fresh pool counts from one again, so a handle that outlived
  a restart could name a new checkout. Save `Pool::issued` and rebuild with
  `Pool::resumed(scope, issued)`, or give each incarnation a new scope.
  `resumed` takes the scope explicitly and restores only the handle sequence:
  every slot is vacant, the clock starts over, every earlier handle is
  `Stale`, and registering the resources again is the caller's job. It is
  `None` when `issued` is beyond `SEQUENCE_MAX`.
- **Checkout accounting.** Outstanding checkouts are counted in a
  `flow_machines::CreditWindow`, one credit per busy slot. The window cannot
  disagree with the slots through this API; its refusals are still mapped
  onto `Refusal` (`Insufficient` as `Busy`, `ExceedsInFlight` as `Stale`)
  rather than asserted, so the pool never panics.

The Ruby gem mirrors `Handle` and `Pool` under the same names; unit variants
are symbols and times are Integer nanoseconds.
