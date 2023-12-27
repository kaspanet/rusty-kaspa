//! Defines a [`MemSizeEstimator`] trait and a companying [`MemMode`] which are used to
//! estimate sizes of run-time objects in memory including deep heap allocations. See
//! struct-level docs for moew details.

use std::{collections::HashSet, mem::size_of, sync::Arc};

use parking_lot::RwLock;

/// The memory mode of the tracked object
#[derive(Debug, Clone, Copy)]
pub enum MemMode {
    Bytes,
    Units,
}

/// The contract for estimating deep memory size owned by this object. Implementors
/// are expected to support only a single function - bytes or units. Objects with pre-compliation
/// known static size or which are containers of items with static size should implement the `_units`
/// estimation and return the number of logical items (usually 1 or the number of items in
/// the container). Objects with varying runtime sizes should implement the `_bytes` estimation.
///
/// By panicking on the remaining unimplemented function we ensure that tests will catch any inconsistency over the
/// used units between the object implementing the contract and the code using its size for various purposes (e.g. cache
/// size tracking)
pub trait MemSizeEstimator {
    /// Estimates the size of this object depending on the passed mem mode
    fn estimate_size(&self, mem_mode: MemMode) -> usize {
        match mem_mode {
            MemMode::Bytes => self.estimate_mem_bytes(),
            MemMode::Units => self.estimate_mem_units(),
        }
    }

    /// Estimates the (deep) size of this object in bytes (including heap owend inner data)
    fn estimate_mem_bytes(&self) -> usize {
        unimplemented!()
    }

    /// Estimates the number of units this object holds in memory where the unit size is usually
    /// a constant known to the caller as well (and hence we avoid computing it over and over)
    fn estimate_mem_units(&self) -> usize {
        unimplemented!()
    }
}

impl MemSizeEstimator for u64 {
    fn estimate_mem_units(&self) -> usize {
        1
    }
}
impl MemSizeEstimator for u32 {
    fn estimate_mem_units(&self) -> usize {
        1
    }
}
impl MemSizeEstimator for u16 {
    fn estimate_mem_units(&self) -> usize {
        1
    }
}
impl MemSizeEstimator for u8 {
    fn estimate_mem_units(&self) -> usize {
        1
    }
}
impl MemSizeEstimator for i64 {
    fn estimate_mem_units(&self) -> usize {
        1
    }
}
impl MemSizeEstimator for i32 {
    fn estimate_mem_units(&self) -> usize {
        1
    }
}
impl MemSizeEstimator for i16 {
    fn estimate_mem_units(&self) -> usize {
        1
    }
}
impl MemSizeEstimator for i8 {
    fn estimate_mem_units(&self) -> usize {
        1
    }
}

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
