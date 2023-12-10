use std::{collections::HashSet, sync::Arc};

use parking_lot::RwLock;

#[derive(Clone, Copy)]
pub enum MemSize {
    Unit,
    UnitDynamic { num_units: usize },
    BytesStatic { num_bytes: usize },
}

impl MemSize {
    pub fn agnostic_size(&self) -> usize {
        match *self {
            MemSize::Unit => 1,
            MemSize::UnitDynamic { num_units } => num_units,
            MemSize::BytesStatic { num_bytes } => num_bytes,
        }
    }

    pub fn unwrap_bytes_static(&self) -> usize {
        match *self {
            MemSize::BytesStatic { num_bytes } => num_bytes,
            _ => panic!("expected bytes static"),
        }
    }
}

pub trait MemSizeEstimator {
    fn estimate_mem_size(&self) -> MemSize {
        MemSize::Unit
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
    fn estimate_mem_size(&self) -> MemSize {
        MemSize::UnitDynamic { num_units: self.len() }
    }
}

impl<T, S> MemSizeEstimator for HashSet<T, S> {
    fn estimate_mem_size(&self) -> MemSize {
        MemSize::UnitDynamic { num_units: self.len() }
    }
}

impl<T: MemSizeEstimator> MemSizeEstimator for Arc<T> {
    fn estimate_mem_size(&self) -> MemSize {
        self.as_ref().estimate_mem_size()
    }
}

impl<T: MemSizeEstimator> MemSizeEstimator for RwLock<T> {
    fn estimate_mem_size(&self) -> MemSize {
        self.read().estimate_mem_size()
    }
}
