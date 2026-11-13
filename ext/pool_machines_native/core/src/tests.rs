#![allow(clippy::expect_used)]
mod model;
mod scope;

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

/// A call that must refuse with `refusal` and leave the pool as it was.
fn refused<const N: usize, T: core::fmt::Debug + PartialEq>(
    pool: &mut Pool<N>,
    refusal: Refusal,
    call: impl FnOnce(&mut Pool<N>) -> Result<T, Refusal>,
) {
    let before = pool.clone();
    assert_eq!(call(pool), Err(refusal));
    assert_eq!(*pool, before, "a refusal changed the pool");
}

#[test]
fn single_use_handles_and_oldest_idle_selection() {
    let mut pool = pool::<2>();
    assert_eq!(pool.register(1, at(0)), Ok(Registration::Registered));
    assert_eq!(pool.register(0, at(1)), Ok(Registration::Registered));
    let first = checked_out(&mut pool, 2);
    assert_eq!(first.slot(), 1);
    assert_eq!(pool.checkin(first, at(3)), Ok(Returned::Idle));
    refused(&mut pool, Refusal::Stale, |p| p.checkin(first, at(3)));
    let other = checked_out(&mut pool, 3);
    assert_eq!(other.slot(), 0);
    let replacement = checked_out(&mut pool, 3);
    assert_eq!(replacement.slot(), 1);
    // The slot is busy again under a new id: the spent handle stays stale.
    refused(&mut pool, Refusal::Stale, |p| p.checkin(first, at(3)));
    assert_eq!(pool.checked_out(), 2);
    assert_eq!(pool.checkout(at(3)), Ok(None));
    assert_eq!(pool.capacity(), 2);
}

#[test]
fn register_refusals_and_the_honoured_repeat() {
    let mut pool = pool::<2>();
    assert_eq!(pool.register(0, at(5)), Ok(Registration::Registered));
    refused(&mut pool, Refusal::UnknownSlot, |p| p.register(2, at(100)));
    refused(&mut pool, Refusal::TimeRegression, |p| p.register(1, at(4)));
    // Regression is checked before the slot is looked at.
    refused(&mut pool, Refusal::TimeRegression, |p| p.register(9, at(4)));
    refused(&mut pool, Refusal::TimeRegression, |p| p.register(0, at(4)));

    // The repeat is honoured and changes nothing: neither the instant the
    // slot went idle nor the observed clock.
    let before = pool.clone();
    assert_eq!(pool.register(0, at(100)), Ok(Registration::AlreadyIdle));
    assert_eq!(pool, before);
    assert_eq!(pool.register(1, at(6)), Ok(Registration::Registered));

    let busy = checked_out(&mut pool, 7);
    assert_eq!(busy.slot(), 0);
    refused(&mut pool, Refusal::Busy, |p| p.register(0, at(8)));
    assert_eq!(pool.begin_drain(), DrainBegin::Begun);
    refused(&mut pool, Refusal::Draining, |p| p.register(1, at(8)));
    refused(&mut pool, Refusal::Draining, |p| p.register(9, at(0)));
}

#[test]
fn draining_retires_returns_without_reclaiming_busy_resources() {
    let mut pool = pool::<2>();
    assert!(!pool.is_draining());
    assert_eq!(pool.register(0, at(0)), Ok(Registration::Registered));
    assert_eq!(pool.register(1, at(0)), Ok(Registration::Registered));
    let busy = checked_out(&mut pool, 1);
    assert_eq!(pool.begin_drain(), DrainBegin::Begun);
    let before = pool.clone();
    assert_eq!(pool.begin_drain(), DrainBegin::AlreadyDraining);
    assert_eq!(pool, before);
    assert!(pool.is_draining());
    refused(&mut pool, Refusal::Draining, |p| p.checkout(at(1)));
    refused(&mut pool, Refusal::Draining, |p| p.register(0, at(1)));
    assert_eq!(pool.retire_idle(at(1), Duration::MAX), Ok(Some(1)));
    assert_eq!(pool.retire_idle(at(1), Duration::ZERO), Ok(None));
    assert_eq!(pool.checkin(busy, at(2)), Ok(Returned::Retire));
    assert_eq!(pool.checked_out(), 0);
    refused(&mut pool, Refusal::Stale, |p| p.checkin(busy, at(2)));
    assert_eq!(pool.retire_idle(at(2), Duration::ZERO), Ok(None));
}

#[test]
fn idle_expiry_picks_the_lowest_old_enough_slot() {
    assert!(Pool::<0>::new().is_none());
    assert!(Pool::<0>::with_scope(1).is_none());
    assert!(Pool::<0>::resumed(1, 0).is_none());
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
    refused(&mut spent, Refusal::Exhausted, |p| p.checkout(at(100)));
    assert_eq!(spent.register(1, at(2)), Ok(Registration::Registered));
    assert_eq!(spent.checked_out(), 0);

    let mut pool = pool::<1>();
    assert_eq!(pool.register(0, at(5)), Ok(Registration::Registered));
    refused(&mut pool, Refusal::TimeRegression, |p| p.checkout(at(4)));
    assert_eq!(pool.begin_drain(), DrainBegin::Begun);
    refused(&mut pool, Refusal::Draining, |p| p.checkout(at(100)));
    assert_eq!(pool.retire_idle(at(5), Duration::ZERO), Ok(Some(0)));
}

/// "Nothing idle" and "nothing old enough" are honoured drain answers about
/// `now`, so they do observe it; only a refusal leaves the clock alone.
#[test]
fn drain_answers_observe_the_clock_and_refusals_do_not() {
    let mut pool = pool::<1>();
    assert_eq!(pool.checkout(at(10)), Ok(None));
    refused(&mut pool, Refusal::TimeRegression, |p| p.register(0, at(9)));
    assert_eq!(pool.register(0, at(10)), Ok(Registration::Registered));
    assert_eq!(pool.retire_idle(at(20), at(60)), Ok(None));
    refused(&mut pool, Refusal::TimeRegression, |p| p.checkout(at(19)));
    refused(&mut pool, Refusal::TimeRegression, |p| {
        p.retire_idle(at(19), Duration::ZERO)
    });
    let handle = checked_out(&mut pool, 20);
    refused(&mut pool, Refusal::TimeRegression, |p| {
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
    refused(&mut pool, Refusal::Stale, |p| p.checkin(handle, at(100)));
    // Stale wins over a regressed clock: the handle names nothing either way.
    refused(&mut pool, Refusal::Stale, |p| p.checkin(handle, at(0)));
    assert!(pool.checkout(at(3)).expect("honoured").is_some());
}

#[test]
fn window_refusals_map_onto_the_pool_refusal() {
    use flow_machines::Refusal as Window;
    assert_eq!(credit(Window::Insufficient { available: 0 }), Refusal::Busy);
    assert_eq!(
        credit(Window::ExceedsInFlight { in_flight: 0 }),
        Refusal::Stale
    );
}

#[test]
fn refusals_display_and_are_errors() {
    use std::string::ToString;
    let all = [
        Refusal::Draining,
        Refusal::TimeRegression,
        Refusal::UnknownSlot,
        Refusal::Busy,
        Refusal::Exhausted,
        Refusal::Stale,
    ];
    for refusal in all {
        let error: &dyn core::error::Error = &refusal;
        assert_ne!(error.to_string(), "");
    }
}
