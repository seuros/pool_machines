#![allow(clippy::expect_used)]
mod model;
mod scope;

use core::assert_matches;

use matryona_machines::testing::{refuses, unchanged};

use super::*;

fn at(t: u64) -> Duration {
    Duration::from_secs(t)
}

fn pool<const N: usize>() -> Pool<N> {
    Pool::new().expect("pool")
}

fn checked_out<const N: usize>(pool: &mut Pool<N>, now: u64) -> Handle {
    pool.checkout(at(now)).expect("honoured").expect("resource")
}

#[test]
fn single_use_handles_and_oldest_idle_selection() {
    let mut pool = pool::<2>();
    assert_eq!(pool.register(1, at(0)), Ok(Registration::Registered));
    assert_eq!(pool.register(0, at(1)), Ok(Registration::Registered));
    let first = checked_out(&mut pool, 2);
    assert_eq!(first.slot(), 1);
    assert_eq!(pool.checkin(first, at(3)), Ok(Returned::Idle));
    refuses(&mut pool, Refusal::Stale, |p| p.checkin(first, at(3)));
    let other = checked_out(&mut pool, 3);
    assert_eq!(other.slot(), 0);
    let replacement = checked_out(&mut pool, 3);
    assert_eq!(replacement.slot(), 1);
    // The slot is busy again under a new id: the spent handle stays stale.
    refuses(&mut pool, Refusal::Stale, |p| p.checkin(first, at(3)));
    assert_eq!(pool.checked_out(), 2);
    assert_eq!(pool.checkout(at(3)), Ok(None));
    assert_eq!(pool.capacity(), 2);
}

#[test]
fn register_refusals_and_the_honoured_repeat() {
    let mut pool = pool::<2>();
    assert_eq!(pool.register(0, at(5)), Ok(Registration::Registered));
    refuses(&mut pool, Refusal::UnknownSlot, |p| p.register(2, at(100)));
    refuses(&mut pool, Refusal::TimeRegression, |p| p.register(1, at(4)));
    // Regression is checked before the slot is looked at.
    refuses(&mut pool, Refusal::TimeRegression, |p| p.register(9, at(4)));
    refuses(&mut pool, Refusal::TimeRegression, |p| p.register(0, at(4)));

    // The repeat is honoured and changes nothing: neither the instant the
    // slot went idle nor the observed clock.
    assert_eq!(
        unchanged(&mut pool, |p| p.register(0, at(100))),
        Ok(Registration::AlreadyIdle)
    );
    assert_eq!(pool.register(1, at(6)), Ok(Registration::Registered));

    let busy = checked_out(&mut pool, 7);
    assert_eq!(busy.slot(), 0);
    refuses(&mut pool, Refusal::Busy, |p| p.register(0, at(8)));
    assert_eq!(pool.begin_drain(), DrainBegin::Begun);
    refuses(&mut pool, Refusal::Draining, |p| p.register(1, at(8)));
    refuses(&mut pool, Refusal::Draining, |p| p.register(9, at(0)));
}

#[test]
fn draining_retires_returns_without_reclaiming_busy_resources() {
    let mut pool = pool::<2>();
    assert!(!pool.is_draining());
    assert_eq!(pool.register(0, at(0)), Ok(Registration::Registered));
    assert_eq!(pool.register(1, at(0)), Ok(Registration::Registered));
    let busy = checked_out(&mut pool, 1);
    assert_eq!(pool.begin_drain(), DrainBegin::Begun);
    assert_eq!(
        unchanged(&mut pool, Pool::begin_drain),
        DrainBegin::AlreadyDraining
    );
    assert!(pool.is_draining());
    refuses(&mut pool, Refusal::Draining, |p| p.checkout(at(1)));
    refuses(&mut pool, Refusal::Draining, |p| p.register(0, at(1)));
    assert_eq!(pool.retire_idle(at(1), Duration::MAX), Ok(Some(1)));
    assert_eq!(pool.retire_idle(at(1), Duration::ZERO), Ok(None));
    assert_eq!(pool.checkin(busy, at(2)), Ok(Returned::Retire));
    assert_eq!(pool.checked_out(), 0);
    refuses(&mut pool, Refusal::Stale, |p| p.checkin(busy, at(2)));
    assert_eq!(pool.retire_idle(at(2), Duration::ZERO), Ok(None));
}

#[test]
fn idle_expiry_picks_the_lowest_old_enough_slot() {
    assert_eq!(Pool::<0>::new(), None);
    assert_eq!(Pool::<0>::with_scope(1), None);
    assert_eq!(Pool::<0>::resumed(1, 0), None);
    let mut pool = pool::<2>();
    assert_eq!(pool.register(1, at(1)), Ok(Registration::Registered));
    assert_eq!(pool.register(0, at(2)), Ok(Registration::Registered));
    assert_eq!(pool.retire_idle(at(3), at(3)), Ok(None));
    assert_eq!(pool.retire_idle(at(4), at(3)), Ok(Some(1)));
    assert_eq!(pool.retire_idle(at(4), at(3)), Ok(None));
    assert_eq!(pool.retire_idle(at(5), at(3)), Ok(Some(0)));
    assert_eq!(pool.register(0, at(5)), Ok(Registration::Registered));
}

/// The defect this crate shipped with: `checkout` recorded `now` before it
/// knew whether it would be honoured, so a refused checkout moved the clock.
#[test]
fn a_refused_checkout_does_not_observe_the_clock() {
    // Exhausted: an idle resource, but no handle id left to issue.
    let mut spent = Pool::<2>::resumed(0, periscope_machines::SEQUENCE_MAX).expect("pool");
    assert_eq!(spent.register(0, at(1)), Ok(Registration::Registered));
    refuses(&mut spent, Refusal::Exhausted, |p| p.checkout(at(100)));
    assert_eq!(spent.register(1, at(2)), Ok(Registration::Registered));
    assert_eq!(spent.checked_out(), 0);

    let mut pool = pool::<1>();
    assert_eq!(pool.register(0, at(5)), Ok(Registration::Registered));
    refuses(&mut pool, Refusal::TimeRegression, |p| p.checkout(at(4)));
    assert_eq!(pool.begin_drain(), DrainBegin::Begun);
    refuses(&mut pool, Refusal::Draining, |p| p.checkout(at(100)));
    assert_eq!(pool.retire_idle(at(5), Duration::ZERO), Ok(Some(0)));
}

/// "Nothing idle" and "nothing old enough" are honoured drain answers about
/// `now`, so they do observe it; only a refusal leaves the clock alone.
#[test]
fn drain_answers_observe_the_clock_and_refusals_do_not() {
    let mut pool = pool::<1>();
    assert_eq!(pool.checkout(at(10)), Ok(None));
    refuses(&mut pool, Refusal::TimeRegression, |p| p.register(0, at(9)));
    assert_eq!(pool.register(0, at(10)), Ok(Registration::Registered));
    assert_eq!(pool.retire_idle(at(20), at(60)), Ok(None));
    refuses(&mut pool, Refusal::TimeRegression, |p| p.checkout(at(19)));
    refuses(&mut pool, Refusal::TimeRegression, |p| {
        p.retire_idle(at(19), Duration::ZERO)
    });
    let handle = checked_out(&mut pool, 20);
    refuses(&mut pool, Refusal::TimeRegression, |p| {
        p.checkin(handle, at(19))
    });
    assert_eq!(pool.checkin(handle, at(20)), Ok(Returned::Idle));
}

#[test]
fn stale_handles_never_advance_the_clock() {
    let mut pool = pool::<1>();
    assert_eq!(pool.register(0, at(0)), Ok(Registration::Registered));
    let handle = checked_out(&mut pool, 1);
    assert_eq!(pool.checkin(handle, at(2)), Ok(Returned::Idle));
    refuses(&mut pool, Refusal::Stale, |p| p.checkin(handle, at(100)));
    // Stale wins over a regressed clock: the handle names nothing either way.
    refuses(&mut pool, Refusal::Stale, |p| p.checkin(handle, at(0)));
    assert_matches!(pool.checkout(at(3)), Ok(Some(_)));
}

/// `Retire` answers the same whatever the time, so it records no clock:
/// two pools that retire the same checkout at different instants are equal.
#[test]
fn a_retiring_checkin_records_no_clock() {
    let mut early = pool::<1>();
    assert_eq!(early.register(0, at(1)), Ok(Registration::Registered));
    let handle = checked_out(&mut early, 2);
    assert_eq!(early.begin_drain(), DrainBegin::Begun);
    let mut late = early.clone();
    assert_eq!(early.checkin(handle, at(2)), Ok(Returned::Retire));
    assert_eq!(late.checkin(handle, at(50)), Ok(Returned::Retire));
    assert_eq!(late, early);
    assert_eq!(late.retire_idle(at(3), Duration::ZERO), Ok(None));
    // An idle checkin does record it.
    let mut idle = pool::<1>();
    assert_eq!(idle.register(0, at(1)), Ok(Registration::Registered));
    let handle = checked_out(&mut idle, 2);
    assert_eq!(idle.checkin(handle, at(50)), Ok(Returned::Idle));
    refuses(&mut idle, Refusal::TimeRegression, |p| p.checkout(at(49)));
}

#[test]
fn refusals_describe_themselves() {
    use std::string::ToString;
    let cases = [
        (Refusal::Draining, "the pool is draining"),
        (
            Refusal::TimeRegression,
            "now is earlier than the observed clock",
        ),
        (
            Refusal::UnknownSlot,
            "slot index is not one of the pool's slots",
        ),
        (Refusal::Busy, "the slot's resource is checked out"),
        (
            Refusal::Exhausted,
            "every handle id of this scope has been issued",
        ),
        (
            Refusal::Stale,
            "the handle names no live checkout of this pool",
        ),
    ];
    for (refusal, text) in cases {
        assert_eq!(refusal.to_string(), text);
    }
}
