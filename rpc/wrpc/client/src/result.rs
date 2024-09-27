//! The [`Result`] type alias bound to the [`Error`](super::error::Error) enum used in this crate.

pub type Result<T> = std::result::Result<T, super::error::Error>;
