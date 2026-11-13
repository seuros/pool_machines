use core::fmt;

/// Why a call was not honoured. A refused call changes nothing: not a slot,
/// not a checkout credit, not an issued handle id, not the observed clock.
///
/// This is the crate's one refusal type. [`Pool::register`](crate::Pool::register)
/// answers [`Draining`](Self::Draining), [`TimeRegression`](Self::TimeRegression),
/// [`UnknownSlot`](Self::UnknownSlot) or [`Busy`](Self::Busy);
/// [`Pool::checkout`](crate::Pool::checkout) answers [`Draining`](Self::Draining),
/// [`TimeRegression`](Self::TimeRegression) or [`Exhausted`](Self::Exhausted);
/// [`Pool::checkin`](crate::Pool::checkin) answers [`Stale`](Self::Stale) or
/// [`TimeRegression`](Self::TimeRegression);
/// [`Pool::retire_idle`](crate::Pool::retire_idle) answers
/// [`TimeRegression`](Self::TimeRegression).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The pool is draining: it registers nothing and checks nothing out.
    Draining,
    /// `now` is earlier than the latest instant the pool observed.
    TimeRegression,
    /// The slot index is not one of the pool's `N` slots.
    UnknownSlot,
    /// The slot's resource is checked out, so nothing can be registered there.
    Busy,
    /// Every handle id of the pool's scope has been issued; none is reissued.
    Exhausted,
    /// The handle names no live checkout of this pool: already checked in,
    /// issued by a pool of another scope, or never issued. The last two are
    /// answered before any slot is looked at.
    Stale,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Draining => "the pool is draining",
            Self::TimeRegression => "now is earlier than the observed clock",
            Self::UnknownSlot => "slot index is not one of the pool's slots",
            Self::Busy => "the slot's resource is checked out",
            Self::Exhausted => "every handle id of this scope has been issued",
            Self::Stale => "the handle names no live checkout of this pool",
        })
    }
}

impl core::error::Error for Refusal {}
