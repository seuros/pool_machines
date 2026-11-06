#![no_std]
#![doc = include_str!("../README.md")]
use core::{num::NonZeroU32, time::Duration};
use flow_machines::CreditWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Vacant,
    Idle(Duration),
    Busy(u64),
}

/// Generation-checked handle within one pool lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle {
    slot: usize,
    generation: u64,
}
impl Handle {
    /// Resource slot to retrieve from caller-owned storage.
    #[must_use]
    pub const fn slot(self) -> usize {
        self.slot
    }
}

/// Check-in disposition for the resource adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Returned {
    /// Retain the resource for another checkout.
    Idle,
    /// Destroy the resource; the pool is draining.
    Retire,
    /// Invalid handle or clock; state was not changed.
    Rejected,
}

/// Slot lifecycle plus bounded checkout accounting.
#[derive(Debug)]
pub struct Pool<const N: usize> {
    slots: [Slot; N],
    credits: CreditWindow,
    generation: u64,
    observed: Duration,
    draining: bool,
}
impl<const N: usize> Pool<N> {
    /// Construct a pool with `1..=u32::MAX` initially vacant slots.
    #[must_use]
    pub fn new() -> Option<Self> {
        let Ok(count) = u32::try_from(N) else {
            return None;
        };
        Some(Self {
            slots: [Slot::Vacant; N],
            credits: CreditWindow::new(NonZeroU32::new(count)?),
            generation: 0,
            observed: Duration::ZERO,
            draining: false,
        })
    }

    /// Register an externally created resource into a vacant slot.
    #[must_use]
    pub fn register(&mut self, slot: usize, now: Duration) -> bool {
        if self.draining || now < self.observed {
            return false;
        }
        let Some(state) = self.slots.get_mut(slot) else {
            return false;
        };
        if *state != Slot::Vacant {
            return false;
        }
        self.observed = now;
        *state = Slot::Idle(now);
        true
    }

    /// Check out the oldest idle resource.
    pub fn checkout(&mut self, now: Duration) -> Option<Handle> {
        if self.draining || !self.observe(now) {
            return None;
        }
        let slot = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, state)| {
                if let Slot::Idle(since) = state {
                    Some((slot, *since))
                } else {
                    None
                }
            })
            .min_by_key(|&(slot, since)| (since, slot))
            .map(|(slot, _)| slot)?;
        let generation = self.generation.checked_add(1)?;
        if !self.credits.try_acquire(NonZeroU32::MIN) {
            return None;
        }
        self.generation = generation;
        self.slots[slot] = Slot::Busy(generation);
        Some(Handle { slot, generation })
    }

    /// Return a live checkout, deciding whether to recycle or retire it.
    ///
    /// # Panics
    /// Panics if internal checkout accounting is inconsistent.
    #[must_use]
    pub fn checkin(&mut self, handle: Handle, now: Duration) -> Returned {
        if now < self.observed {
            return Returned::Rejected;
        }
        let Some(state) = self.slots.get_mut(handle.slot) else {
            return Returned::Rejected;
        };
        if *state != Slot::Busy(handle.generation) {
            return Returned::Rejected;
        }
        self.observed = now;
        assert!(self.credits.release(NonZeroU32::MIN));
        *state = if self.draining {
            Slot::Vacant
        } else {
            Slot::Idle(now)
        };
        if self.draining {
            Returned::Retire
        } else {
            Returned::Idle
        }
    }

    /// Stop future checkout/registration. Existing busy resources are not revoked.
    pub const fn begin_drain(&mut self) {
        self.draining = true;
    }

    /// Retire one idle resource old enough, or any idle resource when draining.
    pub fn retire_idle(&mut self, now: Duration, idle_for: Duration) -> Option<usize> {
        if !self.observe(now) {
            return None;
        }
        let slot = self.slots.iter().position(|state| match state {
            Slot::Idle(since) => {
                self.draining || now.checked_sub(*since).is_some_and(|age| age >= idle_for)
            }
            _ => false,
        })?;
        self.slots[slot] = Slot::Vacant;
        Some(slot)
    }

    /// Number of outstanding checkouts.
    #[must_use]
    pub const fn checked_out(&self) -> u32 {
        self.credits.in_flight()
    }

    fn observe(&mut self, now: Duration) -> bool {
        if now < self.observed {
            return false;
        }
        self.observed = now;
        true
    }
}
#[cfg(test)]
mod tests;
