//! Defines a [`MemSizeEstimator`] trait and a companying [`MemMode`] which are used to
//! estimate sizes of run-time objects in memory, including deep heap allocations. See
//! struct-level docs for more details.

use std::{collections::HashSet, mem::size_of, sync::Arc};

use parking_lot::RwLock;

/// The memory mode of the tracked object
#[derive(Debug, Clone, Copy)]
pub enum MemMode {
    Bytes,
    Units,
    Undefined,
}

/// The contract for estimating deep memory size owned by this object. Implementors
/// are expected to support only a single function - bytes or units. Objects which are
/// containers of items with pre-compilation known static size should implement the `_units`
/// estimation and return the number of logical items (i.e. number of items in
/// the container). Objects with more complex and varying runtime sizes should implement the `_bytes` estimation.
///
/// By panicking on the remaining unimplemented function we ensure that tests will catch any inconsistency over the
/// used units between the object implementing the contract and the code using its size for various purposes (e.g. cache
/// size tracking).
/// Exceptions to the above are objects which delegate the estimation to an underlying inner field (such as Arc or RwLock),
/// which should implement both methods and rely on the inner object to be implemented correctly
pub trait MemSizeEstimator {
    /// Estimates the size of this object depending on the passed mem mode
    fn estimate_size(&self, mem_mode: MemMode) -> usize {
        match mem_mode {
            MemMode::Bytes => self.estimate_mem_bytes(),
            MemMode::Units => self.estimate_mem_units(),
            MemMode::Undefined => unimplemented!(),
        }
    }

    /// Estimates the (deep) size of this object in bytes (including heap owned inner data)
    fn estimate_mem_bytes(&self) -> usize {
        unimplemented!()
    }

    /// Estimates the number of units this object holds in memory where the unit byte size is usually
    /// a constant known to the caller as well (and hence we avoid computing it over and over)
    fn estimate_mem_units(&self) -> usize {
        unimplemented!()
    }
}

impl MemSizeEstimator for u64 {}
impl MemSizeEstimator for u32 {}
impl MemSizeEstimator for u16 {}
impl MemSizeEstimator for u8 {}
impl MemSizeEstimator for i64 {}
impl MemSizeEstimator for i32 {}
impl MemSizeEstimator for i16 {}
impl MemSizeEstimator for i8 {}

impl<T> MemSizeEstimator for Vec<T> {
    fn estimate_mem_units(&self) -> usize {
        self.len()
    }
}

impl<T, S> MemSizeEstimator for HashSet<T, S> {
    fn estimate_mem_units(&self) -> usize {
        self.len()
    }
}

impl<T: MemSizeEstimator> MemSizeEstimator for Arc<T> {
    fn estimate_mem_bytes(&self) -> usize {
        self.as_ref().estimate_mem_bytes() + size_of::<Self>()
    }

    fn estimate_mem_units(&self) -> usize {
        self.as_ref().estimate_mem_units()
    }
}

impl<T: MemSizeEstimator> MemSizeEstimator for RwLock<T> {
    fn estimate_mem_bytes(&self) -> usize {
        self.read().estimate_mem_bytes() + size_of::<Self>()
    }

    fn estimate_mem_units(&self) -> usize {
        self.read().estimate_mem_units()
    }
}
