//! Scopes, the handle sequence, and restarts.

use periscope_machines::{SEQUENCE_MAX, scoped};

use super::*;

#[test]
fn handles_carry_the_scope_and_a_sequence_that_never_repeats() {
    let mut pool = Pool::<1>::with_scope(7).expect("pool");
    assert_eq!((pool.scope(), pool.issued()), (7, 0));
    assert_eq!(pool.register(0, at(0)), Ok(Registration::Registered));
    let first = checked_out(&mut pool, 1);
    assert_eq!(
        (first.scope(), first.slot(), first.key),
        (7, 0, scoped(7, 1))
    );
    assert_eq!(pool.checkin(first, at(1)), Ok(Returned::Idle));
    let second = checked_out(&mut pool, 1);
    assert_eq!(second.key, scoped(7, 2));
    assert_eq!(pool.issued(), 2);
    assert_eq!(Pool::<1>::new().expect("pool").scope(), 0);
}

#[test]
fn a_handle_of_another_scope_or_never_issued_is_stale() {
    let mut a = Pool::<1>::with_scope(1).expect("pool");
    let mut b = Pool::<1>::with_scope(2).expect("pool");
    for pool in [&mut a, &mut b] {
        assert_eq!(pool.register(0, at(0)), Ok(Registration::Registered));
    }
    let from_a = checked_out(&mut a, 1);
    let from_b = checked_out(&mut b, 1);
    // Same slot, same sequence number, different scope.
    refuses(&mut b, Refusal::Stale, |p| p.checkin(from_a, at(2)));
    refuses(&mut a, Refusal::Stale, |p| p.checkin(from_b, at(2)));
    // Forged: a sequence the pool has not reached, sequence zero, a slot it lacks.
    let ahead = Handle {
        slot: 0,
        key: scoped(1, 2),
    };
    let zero = Handle {
        slot: 0,
        key: scoped(1, 0),
    };
    let elsewhere = Handle {
        slot: 9,
        key: from_a.key,
    };
    for forged in [ahead, zero, elsewhere] {
        refuses(&mut a, Refusal::Stale, |p| p.checkin(forged, at(2)));
    }
    assert_eq!(a.checkin(from_a, at(2)), Ok(Returned::Idle));
    assert_eq!(b.checkin(from_b, at(2)), Ok(Returned::Idle));
}

#[test]
fn two_pools_in_one_scope_alias_which_is_why_scopes_exist() {
    let mut a = Pool::<1>::new().expect("pool");
    let mut b = Pool::<1>::new().expect("pool");
    for pool in [&mut a, &mut b] {
        assert_eq!(pool.register(0, at(0)), Ok(Registration::Registered));
    }
    let from_a = checked_out(&mut a, 1);
    assert_eq!(checked_out(&mut b, 1), from_a);
}

#[test]
fn a_resumed_pool_continues_the_sequence_and_refuses_earlier_handles() {
    let mut pool = Pool::<1>::with_scope(3).expect("pool");
    assert_eq!(pool.register(0, at(0)), Ok(Registration::Registered));
    let before = checked_out(&mut pool, 1);
    let saved = pool.issued();

    let mut resumed = Pool::<1>::resumed(3, saved).expect("pool");
    assert_eq!(
        (resumed.scope(), resumed.issued(), resumed.checked_out()),
        (3, 1, 0)
    );
    // Nothing is registered; the clock starts over.
    assert_eq!(resumed.checkout(at(0)), Ok(None));
    refuses(&mut resumed, Refusal::Stale, |p| p.checkin(before, at(0)));
    assert_eq!(resumed.register(0, at(0)), Ok(Registration::Registered));
    let after = checked_out(&mut resumed, 0);
    assert_eq!(after.key, scoped(3, 2));
    // Slot 0 is busy again, but under another id.
    refuses(&mut resumed, Refusal::Stale, |p| p.checkin(before, at(0)));

    assert!(Pool::<1>::resumed(3, SEQUENCE_MAX).is_some());
    assert!(Pool::<1>::resumed(3, SEQUENCE_MAX + 1).is_none());
}

#[test]
fn the_last_id_of_a_scope_is_issued_once_and_then_the_scope_is_exhausted() {
    let mut pool = Pool::<2>::resumed(4, SEQUENCE_MAX - 1).expect("pool");
    assert_eq!(pool.register(0, at(0)), Ok(Registration::Registered));
    assert_eq!(pool.register(1, at(0)), Ok(Registration::Registered));
    let last = checked_out(&mut pool, 1);
    assert_eq!(last.key, scoped(4, SEQUENCE_MAX));
    refuses(&mut pool, Refusal::Exhausted, |p| p.checkout(at(50)));
    assert_eq!(pool.checked_out(), 1);
    // Nothing idle is still an honoured answer on a spent scope.
    let busy = pool.clone();
    assert_eq!(pool.checkin(last, at(1)), Ok(Returned::Idle));
    refuses(&mut pool, Refusal::Exhausted, |p| p.checkout(at(50)));
    assert_ne!(pool, busy);
}
