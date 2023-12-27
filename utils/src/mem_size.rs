use std::{collections::HashSet, mem::size_of, sync::Arc};

use parking_lot::RwLock;

#[derive(Debug, Clone, Copy)]
pub enum MemMode {
    Bytes,
    Units,
}

pub trait MemSizeEstimator {
    fn estimate_size(&self, mem_mode: MemMode) -> usize {
        match mem_mode {
            MemMode::Bytes => self.estimate_mem_bytes(),
            MemMode::Units => self.estimate_mem_units(),
        }
    }

    fn estimate_mem_bytes(&self) -> usize {
        unimplemented!()
    }

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
