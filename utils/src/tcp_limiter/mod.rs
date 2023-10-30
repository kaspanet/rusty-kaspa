mod tcp_wrapper;

use std::ops::Deref;
use std::sync::atomic::AtomicI32;

pub use tcp_wrapper::Wrapper;

/// `Limit` is a struct that tracks the current number of active TCP connections
/// against a maximum limit. It is designed to be shared across different services
/// like gRPC, JSON RPC, REST, etc., to provide a unified connection limiting mechanism.
///
/// # Fields
/// - `max`: The maximum number of allowed active TCP connections.
/// - `current`: The current number of active TCP connections.
///
/// # Usage
/// The `Limit` struct is meant to be wrapped in an `Arc` and shared between instances
#[derive(Debug)]
pub struct Limit {
    max: i32,
    current: AtomicI32,
}

impl Limit {
    pub fn new(max: i32) -> Self {
        Self { max, current: Default::default() }
    }
    #[inline]
    pub fn max(&self) -> i32 {
        self.max
    }
}

impl Deref for Limit {
    type Target = AtomicI32;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.current
    }
}
