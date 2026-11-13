//! Native resource roster. Ruby resources are not retained or operated here.
mod init;
mod pool;

use std::time::Duration;

use magnus::{Error, Integer, Ruby, TryConvert, Value};

#[derive(Clone, Copy)]
struct Count(u64);

impl TryConvert for Count {
    fn try_convert(value: Value) -> Result<Self, Error> {
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                Ruby::get_with(value).exception_type_error(),
                "slot/time must be an unsigned 64-bit Integer (time in nanoseconds)",
            ));
        }
        u64::try_convert(value).map(Self)
    }
}

impl Count {
    const fn duration(self) -> Duration {
        Duration::from_nanos(self.0)
    }

    /// A slot index. One beyond the address space names no slot, like any
    /// other index past the pool's last: the core refuses it.
    fn slot(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

/// An issuer scope: an Integer in `0..=65535`.
#[derive(Clone, Copy)]
struct Scope(u16);

impl TryConvert for Scope {
    fn try_convert(value: Value) -> Result<Self, Error> {
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                Ruby::get_with(value).exception_type_error(),
                "expected an Integer scope (0..=65535)",
            ));
        }
        u16::try_convert(value).map(Self)
    }
}
