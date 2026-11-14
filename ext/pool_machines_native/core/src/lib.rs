#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(test)]
extern crate std;

mod refusal;

use core::time::Duration;

use periscope_machines::{Issuer, scope_of};

pub use refusal::Refusal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Vacant,
    /// Idle since the instant it was registered or checked in.
    Idle(Duration),
    /// Checked out under the handle id it carries.
    Busy(u64),
}

/// One checkout: the slot it borrowed and the scoped id it was issued under.
///
/// Only the pool that issued it honours it, and only once: an id is never
/// reissued, so a handle that was checked in is [`Refusal::Stale`] forever,
/// also after its slot is checked out again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    slot: usize,
    /// Scoped id from the pool's issuer.
    key: u64,
}

impl Handle {
    /// Resource slot to retrieve from caller-owned storage.
    #[must_use]
    pub const fn slot(self) -> usize {
        self.slot
    }

    /// The scope of the pool that issued this handle, see [`Pool::with_scope`].
    #[must_use]
    pub const fn scope(self) -> u16 {
        scope_of(self.key)
    }
}

/// Honoured outcome of [`Pool::register`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// The vacant slot now holds an idle resource.
    Registered,
    /// An idempotent repeat: the slot already holds an idle resource. Nothing
    /// changed, not the instant it went idle and not the observed clock.
    AlreadyIdle,
}

/// Honoured outcome of [`Pool::checkin`]: what the adapter does with the resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Returned {
    /// Retain the resource for another checkout.
    Idle,
    /// Destroy the resource; the pool is draining and the slot is vacant.
    Retire,
}

/// Honoured outcome of [`Pool::begin_drain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainBegin {
    /// The pool is now draining.
    Begun,
    /// An idempotent repeat: the pool was already draining. Nothing changed.
    AlreadyDraining,
}

/// Slot lifecycle: each slot is vacant, idle, or checked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pool<const N: usize> {
    slots: [Slot; N],
    issuer: Issuer,
    observed: Duration,
    draining: bool,
}

impl<const N: usize> Pool<N> {
    /// A pool of `N` vacant slots in scope zero, with no handle issued.
    ///
    /// `None` unless `N` is in `1..=u32::MAX`.
    #[must_use]
    pub fn new() -> Option<Self> {
        Self::with_scope(0)
    }

    /// A pool of `N` vacant slots whose handles carry `scope`.
    ///
    /// Handles are only meaningful to the pool that issued them. Two pools in
    /// the same scope issue the same handle ids, so give every pool whose
    /// handles could meet its own scope. `None` unless `N` is in `1..=u32::MAX`.
    #[must_use]
    pub fn with_scope(scope: u16) -> Option<Self> {
        Self::build(Issuer::new(scope))
    }

    /// A pool that continues `scope` after a restart: `issued` is what
    /// [`Pool::issued`] answered when the caller saved it.
    ///
    /// Only the handle sequence is restored. Every slot is vacant, the clock
    /// starts over, and every handle issued before the restart is
    /// [`Refusal::Stale`]; registering the resources again is the caller's
    /// job. The next handle carries sequence `issued + 1`, so none is reissued
    /// as long as the saved count is not lower than what was really issued:
    /// save it before handing a handle out.
    ///
    /// `None` unless `N` is in `1..=u32::MAX`, and when `issued` is beyond
    /// `periscope_machines::SEQUENCE_MAX`, which no pool reaches.
    #[must_use]
    pub fn resumed(scope: u16, issued: u64) -> Option<Self> {
        Self::build(Issuer::resumed(scope, issued)?)
    }

    /// `N` is in `1..=u32::MAX`, so [`Pool::checked_out`] always fits a `u32`.
    fn build(issuer: Issuer) -> Option<Self> {
        if !u32::try_from(N).is_ok_and(|count| count > 0) {
            return None;
        }
        Some(Self {
            slots: [Slot::Vacant; N],
            issuer,
            observed: Duration::ZERO,
            draining: false,
        })
    }

    /// The scope carried by every handle this pool issues.
    #[must_use]
    pub const fn scope(&self) -> u16 {
        self.issuer.scope()
    }

    /// Handles issued in this scope so far, live or not: the count to save
    /// for [`Pool::resumed`].
    #[must_use]
    pub const fn issued(&self) -> u64 {
        self.issuer.issued()
    }

    /// Compiled slot count `N`.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Number of outstanding checkouts: the busy slots. O(N).
    #[must_use]
    pub fn checked_out(&self) -> u32 {
        let busy = self
            .slots
            .iter()
            .filter(|state| matches!(state, Slot::Busy(_)))
            .count();
        // Every constructor holds N to `1..=u32::MAX`, so the count fits.
        u32::try_from(busy).unwrap_or(u32::MAX)
    }

    /// Whether [`Pool::begin_drain`] was called.
    #[must_use]
    pub const fn is_draining(&self) -> bool {
        self.draining
    }

    /// Register an externally created resource into a vacant slot, idle since `now`.
    ///
    /// Answers [`Registration::AlreadyIdle`] when the slot already holds an
    /// idle resource: the effect asked for holds and the slot proves it, so
    /// the repeat is honoured and changes nothing.
    ///
    /// # Errors
    ///
    /// Checked in this order: [`Refusal::Draining`], [`Refusal::TimeRegression`],
    /// [`Refusal::UnknownSlot`] for an index of `N` or more, and
    /// [`Refusal::Busy`] when the slot's resource is checked out.
    pub fn register(&mut self, slot: usize, now: Duration) -> Result<Registration, Refusal> {
        if self.draining {
            return Err(Refusal::Draining);
        }
        self.check_time(now)?;
        match self.slots.get_mut(slot) {
            None => Err(Refusal::UnknownSlot),
            Some(Slot::Busy(_)) => Err(Refusal::Busy),
            Some(Slot::Idle(_)) => Ok(Registration::AlreadyIdle),
            Some(state @ Slot::Vacant) => {
                *state = Slot::Idle(now);
                self.observed = now;
                Ok(Registration::Registered)
            }
        }
    }

    /// Check out the resource that has been idle longest, lowest slot first
    /// among equals.
    ///
    /// A drain over the idle slots: `None` is the honoured answer "nothing is
    /// idle at `now`". Honoured either way, so `now` is recorded as observed
    /// either way and an earlier instant is refused afterwards.
    ///
    /// # Errors
    ///
    /// [`Refusal::Draining`], [`Refusal::TimeRegression`], or
    /// [`Refusal::Exhausted`] when a resource is idle but all
    /// `periscope_machines::SEQUENCE_MAX` (2^48 - 1) handle ids of the pool's
    /// scope have been issued. A refusal records nothing, `now` included.
    pub fn checkout(&mut self, now: Duration) -> Result<Option<Handle>, Refusal> {
        if self.draining {
            return Err(Refusal::Draining);
        }
        self.check_time(now)?;
        let oldest = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, state)| match state {
                Slot::Idle(since) => Some((*since, slot)),
                Slot::Vacant | Slot::Busy(_) => None,
            })
            .min();
        let Some((_, slot)) = oldest else {
            self.observed = now;
            return Ok(None);
        };
        // Last check: an id is only spent on a checkout that is honoured.
        let key = match self.issuer.issue() {
            Ok(key) => key,
            Err(periscope_machines::Refusal::Exhausted) => return Err(Refusal::Exhausted),
        };
        self.observed = now;
        self.slots[slot] = Slot::Busy(key);
        Ok(Some(Handle { slot, key }))
    }

    /// Return a live checkout, deciding whether to recycle or retire it.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`] for a handle that names no live checkout here: one
    /// already checked in, one of another scope, or one never issued (the
    /// last two before any slot is read). A repeated `checkin` is never
    /// honoured: the pool keeps nothing about a checkout once it is returned.
    /// [`Refusal::TimeRegression`] for a live handle and an earlier `now`.
    ///
    /// [`Returned::Idle`] records `now`: the slot is idle since `now`.
    /// [`Returned::Retire`] does not: retiring answers the same whatever the
    /// time, so a later call may still show an instant before this one.
    pub fn checkin(&mut self, handle: Handle, now: Duration) -> Result<Returned, Refusal> {
        if !self.issuer.owns(handle.key) {
            return Err(Refusal::Stale);
        }
        if self.slots.get(handle.slot) != Some(&Slot::Busy(handle.key)) {
            return Err(Refusal::Stale);
        }
        self.check_time(now)?;
        if self.draining {
            // Retiring holds whatever the time: the clock is not recorded.
            self.slots[handle.slot] = Slot::Vacant;
            Ok(Returned::Retire)
        } else {
            self.observed = now;
            self.slots[handle.slot] = Slot::Idle(now);
            Ok(Returned::Idle)
        }
    }

    /// Stop future checkout and registration. Busy resources are not revoked.
    ///
    /// Never refuses. Repeating it answers [`DrainBegin::AlreadyDraining`].
    pub const fn begin_drain(&mut self) -> DrainBegin {
        if self.draining {
            return DrainBegin::AlreadyDraining;
        }
        self.draining = true;
        DrainBegin::Begun
    }

    /// Retire one resource idle for at least `idle_for`, or any idle resource
    /// when draining; lowest slot first. The slot is vacant afterwards.
    ///
    /// A drain: `None` is the honoured answer "nothing is old enough at
    /// `now`". Honoured either way, so `now` is recorded as observed either
    /// way and an earlier instant is refused afterwards.
    ///
    /// # Errors
    ///
    /// [`Refusal::TimeRegression`]; nothing is recorded.
    pub fn retire_idle(
        &mut self,
        now: Duration,
        idle_for: Duration,
    ) -> Result<Option<usize>, Refusal> {
        self.check_time(now)?;
        self.observed = now;
        let draining = self.draining;
        let slot = self.slots.iter().position(|state| match state {
            Slot::Idle(since) => draining || now.saturating_sub(*since) >= idle_for,
            Slot::Vacant | Slot::Busy(_) => false,
        });
        if let Some(slot) = slot {
            self.slots[slot] = Slot::Vacant;
        }
        Ok(slot)
    }

    /// Time regression is checked without recording: `observed` moves only
    /// when the call is honoured.
    const fn check_time(&self, now: Duration) -> Result<(), Refusal> {
        if now.as_nanos() < self.observed.as_nanos() {
            return Err(Refusal::TimeRegression);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
