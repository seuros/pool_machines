//! Native resource roster. Ruby resources are not retained or operated here.
mod init;
mod pool;

use std::{fmt::Display, time::Duration};

use magnus::{Error, Integer, Ruby, TryConvert, Value, prelude::*};

/// `value` as an Integer, `TypeError` naming the argument otherwise: Magnus
/// would truncate Floats and parse nothing else usefully.
fn integer(value: Value, name: &str) -> Result<(), Error> {
    if Integer::from_value(value).is_some() {
        return Ok(());
    }
    Err(Error::new(
        Ruby::get_with(value).exception_type_error(),
        format!("{name} must be an Integer, not {}", value.class()),
    ))
}

/// An Integer converted to `T`, `RangeError` naming the argument and the
/// range `0..=max` when it does not fit.
fn unsigned<T: TryConvert>(value: Value, name: &str, max: impl Display) -> Result<T, Error> {
    integer(value, name)?;
    T::try_convert(value).map_err(|_| {
        Error::new(
            Ruby::get_with(value).exception_range_error(),
            format!("{name} must be in 0..={max}"),
        )
    })
}

/// A slot index: any Integer a `usize` holds. One past the pool's last slot
/// is the core's `UnknownSlot`; one the platform cannot hold is `RangeError`.
pub(crate) fn slot(value: Value) -> Result<usize, Error> {
    unsigned(value, "slot", usize::MAX)
}

/// An instant or span: Integer nanoseconds in `0..=u64::MAX`.
pub(crate) fn nanos(value: Value, name: &str) -> Result<Duration, Error> {
    unsigned(value, name, u64::MAX).map(Duration::from_nanos)
}

/// A handle count saved from `issued`: an Integer in `0..=u64::MAX`.
pub(crate) fn count(value: Value, name: &str) -> Result<u64, Error> {
    unsigned(value, name, u64::MAX)
}

/// An issuer scope: an Integer in `0..=u16::MAX`.
pub(crate) fn scope(value: Value) -> Result<u16, Error> {
    unsigned(value, "scope", u16::MAX)
}
