//!
//! # Consensus Core
//!
//! This crate implements primitives used in the Kaspa node consensus processing.
//!

#![cfg_attr(not(feature = "std"), no_std)]
// Without `atomic-mass` the mass field is `Cell`-backed, so `Transaction` is `!Sync`
// by design (see `MassInner`); `Arc<Transaction>` on single-threaded targets is fine.
#![cfg_attr(not(feature = "atomic-mass"), allow(clippy::arc_with_non_send_sync))]
// Without `std`, several `match`es lose their `#[cfg(feature = "std")]` arm and collapse
// to a single infallible arm.
#![cfg_attr(not(feature = "std"), allow(clippy::infallible_destructuring_match))]

extern crate alloc;
extern crate core;
extern crate self as consensus_core;

use alloc::vec::Vec;
use core::hash::{BuildHasher, Hasher};

pub use kaspa_hashes::Hash;

// Re-export `HashMap`/`HashSet` from a single place so the whole crate (and its
// consumers) share one hasher policy
#[cfg(feature = "std")]
pub type HashMap<K, V, S = std::hash::RandomState> = hashbrown::HashMap<K, V, S>;
#[cfg(feature = "std")]
pub type HashSet<T, S = std::hash::RandomState> = hashbrown::HashSet<T, S>;
#[cfg(not(feature = "std"))]
pub use hashbrown::{HashMap, HashSet};

pub mod acceptance_data;
pub mod api;
pub mod block;
pub mod blockhash;
pub mod blockstatus;
pub mod coinbase;
pub mod config;
pub mod constants;
pub mod daa_score_timestamp;
pub mod errors;
pub mod hashing;
pub mod header;
pub mod mass;
pub mod merkle;
pub mod mining_rules;
pub mod muhash;
pub mod network;
pub mod pruning;
#[cfg(feature = "sign")]
pub mod sign;
pub mod subnets;
pub mod trusted;
pub mod tx;
pub mod utxo;

/// Integer type for accumulated PoW of blue blocks. We expect no more than
/// 2^128 work in a single block (btc has ~2^80), and no more than 2^64
/// overall blocks, so 2^192 is definitely a justified upper-bound.
pub type BlueWorkType = kaspa_math::Uint192;

/// The extends directly from the expectation above about having no more than
/// 2^128 work in a single block
pub const MAX_WORK_LEVEL: BlockLevel = 128;

/// The type used to represent the GHOSTDAG K parameter
pub type KType = u16;

/// Map from Block hash to K type
pub type HashKTypeMap = alloc::sync::Arc<BlockHashMap<KType>>;

/// This HashMap skips the hashing of the key and uses the key directly as the hash.
/// Should only be used for block hashes that have correct DAA,
/// otherwise it is susceptible to DOS attacks via hash collisions.
pub type BlockHashMap<V> = HashMap<Hash, V, BlockHasher>;

/// Same as `BlockHashMap` but a `HashSet`.
pub type BlockHashSet = HashSet<Hash, BlockHasher>;

pub trait HashMapCustomHasher {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
}

// HashMap::new and HashMap::with_capacity are only implemented on hashbrown's
// DefaultHashBuilder, so we provide our own versions for the BlockHasher aliases.
impl<V> HashMapCustomHasher for BlockHashMap<V> {
    #[inline(always)]
    fn new() -> Self {
        Self::with_hasher(BlockHasher::new())
    }
    #[inline(always)]
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity_and_hasher(cap, BlockHasher::new())
    }
}

impl HashMapCustomHasher for BlockHashSet {
    #[inline(always)]
    fn new() -> Self {
        Self::with_hasher(BlockHasher::new())
    }
    #[inline(always)]
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity_and_hasher(cap, BlockHasher::new())
    }
}

#[derive(Default, Debug)]
pub struct ChainPath {
    pub added: Vec<Hash>,
    pub removed: Vec<Hash>,
}

/// `hashes::Hash` writes 4 u64s so we just use the last one as the hash here
#[derive(Default, Clone, Copy)]
pub struct BlockHasher(u64);

impl BlockHasher {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(0)
    }
}

impl Hasher for BlockHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline(always)]
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
    #[cold]
    fn write(&mut self, _: &[u8]) {
        unimplemented!("use write_u64")
    }
}

impl BuildHasher for BlockHasher {
    type Hasher = Self;

    #[inline(always)]
    fn build_hasher(&self) -> Self::Hasher {
        Self(0)
    }
}

pub type BlockLevel = u8;

#[cfg(test)]
mod tests {
    use super::BlockHasher;
    use core::hash::{Hash as _, Hasher as _};
    use kaspa_hashes::Hash;
    #[test]
    fn test_block_hasher() {
        let hash = Hash::from_le_u64([1, 2, 3, 4]);
        let mut hasher = BlockHasher::default();
        hash.hash(&mut hasher);
        assert_eq!(hasher.finish(), 4);
    }
}
