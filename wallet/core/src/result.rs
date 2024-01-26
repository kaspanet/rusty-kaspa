//!
//! [`Result`] type alias bound to the framework [`Error`](crate::error::Error) enum.
//!

/// [`Result`] type alias bound to the framework [`Error`](crate::error::Error) enum.
pub type Result<T, E = super::error::Error> = std::result::Result<T, E>;
