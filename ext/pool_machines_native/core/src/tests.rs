#![allow(clippy::expect_used)]
use super::*;
fn at(t: u64) -> Duration {
    Duration::from_secs(t)
}

#[test]
fn generation_checked_returns_and_oldest_idle_selection() {
    let mut pool = Pool::<2>::new().expect("pool");
    assert!(pool.register(1, at(0)));
    assert!(pool.register(0, at(1)));
    let first = pool.checkout(at(2)).expect("resource");
    assert_eq!(first.slot(), 1);
    assert_eq!(pool.checkin(first, at(3)), Returned::Idle);
    assert_eq!(pool.checkin(first, at(3)), Returned::Rejected);
    let other = pool.checkout(at(3)).expect("resource");
    assert_eq!(other.slot(), 0);
    let replacement = pool.checkout(at(3)).expect("resource");
    assert_eq!(replacement.slot(), 1);
    assert_eq!(pool.checkin(first, at(3)), Returned::Rejected);
    assert_eq!(pool.checked_out(), 2);
    assert_eq!(pool.checkout(at(3)), None);
}

#[test]
fn draining_retires_returns_without_reclaiming_busy_resources() {
    let mut pool = Pool::<2>::new().expect("pool");
    assert!(pool.register(0, at(0)));
    assert!(pool.register(1, at(0)));
    let busy = pool.checkout(at(1)).expect("resource");
    pool.begin_drain();
    assert_eq!(pool.checkout(at(1)), None);
    assert!(!pool.register(0, at(1)));
    assert_eq!(pool.retire_idle(at(1), Duration::MAX), Some(1));
    assert_eq!(pool.checkin(busy, at(2)), Returned::Retire);
    assert_eq!(pool.checked_out(), 0);
}

#[test]
fn idle_expiry_clock_regression_and_exhaustion_are_explicit() {
    assert!(Pool::<0>::new().is_none());
    let mut pool = Pool::<1>::new().expect("pool");
    assert!(pool.register(0, at(1)));
    assert_eq!(pool.retire_idle(at(3), at(3)), None);
    assert_eq!(pool.retire_idle(at(4), at(3)), Some(0));
    assert!(!pool.register(0, at(3)));
    assert!(pool.register(0, at(4)));
    pool.generation = u64::MAX;
    assert_eq!(pool.checkout(at(4)), None);
    assert_eq!(pool.checked_out(), 0);
}

#[test]
fn rejected_stale_handles_never_advance_the_clock() {
    let mut pool = Pool::<1>::new().expect("pool");
    assert!(pool.register(0, at(0)));
    let handle = pool.checkout(at(1)).expect("checkout");
    assert_eq!(pool.checkin(handle, at(2)), Returned::Idle);
    assert_eq!(pool.checkin(handle, at(100)), Returned::Rejected);
    assert!(pool.checkout(at(3)).is_some());
}

#[test]
fn invalid_registration_does_not_consume_clock_progress() {
    let mut pool = Pool::<1>::new().expect("pool");
    assert!(!pool.register(1, at(100)));
    assert!(pool.register(0, at(1)));
    assert!(!pool.register(0, at(100)));
    assert!(pool.checkout(at(2)).is_some());
}
