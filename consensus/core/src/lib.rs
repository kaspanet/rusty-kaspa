use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};

use hashes::Hash;
use tx::TransactionOutpoint;

pub mod api;
pub mod block;
pub mod blockhash;
pub mod blockstatus;
pub mod coinbase;
pub mod constants;
pub mod errors;
pub mod hashing;
pub mod header;
pub mod mass;
pub mod merkle;
pub mod muhash;
pub mod networktype;
pub mod notify;
pub mod pruning;
pub mod sign;
pub mod subnets;
pub mod tx;
pub mod utxo;

pub mod testutils;

/// Integer type for accumulated PoW of blue blocks. We expect no more than
/// 2^128 work in a single block (btc has ~2^80), and no more than 2^64
/// overall blocks, so 2^192 is definitely a justified upper-bound.
pub type BlueWorkType = math::Uint192;

/// This HashMap skips the hashing of the key and uses the key directly as the hash.
/// Should only be used for block hashes that have correct DAA,
/// otherwise it is susceptible to DOS attacks via hash collisions.
pub type BlockHashMap<V> = HashMap<Hash, V, BlockHasher>;

/// This HashMap skips the hashing of the TransactionOuoint and uses the TransactionId and  TransactionIndex directly as the hash.
/// TODO: some comment on when applicable / hash collistion properties.
pub type TransactionOutpointHashMap<V> = HashMap<TransactionOutpoint, V, TransactionOutpointHasher>;

/// Same as `BlockHashMap` but a `HashSet`.
pub type BlockHashSet = HashSet<Hash, BlockHasher>;

pub trait HashMapCustomHasher {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
}

// HashMap::new and HashMap::with_capacity are only implemented on Hasher=RandomState
// to avoid type inference problems, so we need to provide our own versions.
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

impl<V> HashMapCustomHasher for TransactionOutpointHashMap<V> {
    #[inline(always)]
    fn new() -> Self {
        Self::with_hasher(TransactionOutpointHasher::new())
    }
    #[inline(always)]
    fn with_capacity(cap: usize) -> Self {
        Self::with_capacity_and_hasher(cap, TransactionOutpointHasher::new())
    }
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

/// `TransactionOutpoint` consists of 4 u64s of TransactionId as well as one u32 TransactionIndex,
/// as such we xor all u64 of the TransactionId, and cast the last u32 index as u64 and xor it in as well.
#[derive(Default, Clone, Copy)]
pub struct TransactionOutpointHasher(u64);

impl TransactionOutpointHasher {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(0)
    }
}

impl Hasher for TransactionOutpointHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline(always)]
    fn write_u64(&mut self, v: u64) {
        self.0 ^= v;
    }

    #[cold]
    fn write(&mut self, _: &[u8]) {
        unimplemented!("use write_u64")
    }
}

impl BuildHasher for TransactionOutpointHasher {
    type Hasher = Self;

    #[inline(always)]
    fn build_hasher(&self) -> Self::Hasher {
        Self(0)
    }
}

pub type BlockLevel = u8;

#[cfg(test)]
mod tests {
    use crate::tx::{TransactionId, TransactionOutpoint};

    use super::BlockHasher;
    use super::TransactionOutpointHasher;
    use hashes::Hash;
    use std::hash::{Hash as _, Hasher as _};

    #[test]
    fn test_block_hasher() {
        let hash = Hash::from_le_u64([1, 2, 3, 4]);
        let mut hasher = BlockHasher::default();
        hash.hash(&mut hasher);
        assert_eq!(hasher.finish(), 4);
    }

    #[test]
    fn test_outpoint_hasher() {
        let transaction_outpoint = TransactionOutpoint::new(TransactionId::from_le_u64([12345, 24567, 54321, 11111]), 5000);
        let mut hasher = TransactionOutpointHasher::default();
        transaction_outpoint.hash(&mut hasher);
        let expected: u64 = (((12345_u64 ^ 24567_u64) ^ 54321_u64) ^ 11111_u64) ^ 5000_u64;
        assert_eq!(hasher.finish(), expected);
    }
}
