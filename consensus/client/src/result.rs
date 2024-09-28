//! [`Result`] type alias that is bound to the [`Error`](super::error::Error) type from this crate.

pub type Result<T, E = super::error::Error> = std::result::Result<T, E>;
