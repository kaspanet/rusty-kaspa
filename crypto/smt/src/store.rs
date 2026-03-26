//! Storage trait and in-memory implementation for the Sparse Merkle Tree.
//!
//! [`SmtStore`] is a **read-only** trait for branch node lookups, enabling both
//! in-memory (`BTreeSmtStore`) and persistent (e.g. RocksDB) backends.
//!
//! Branch nodes are keyed by [`BranchKey`] `(height, node_key)` and store
//! the `(left, right)` child hashes.

use alloc::collections::BTreeMap;
use core::convert::Infallible;

use kaspa_hashes::{Hash, ZERO_HASH};

/// Key for a branch node in the sparse Merkle tree.
///
/// - `height`: level from the leaf. Height 0 = parent of two leaves (depth 255),
///   height 255 = the root node (depth 0).
/// - `node_key`: the leaf key with bits at big-endian positions ≥ `(256 - height)`
///   zeroed out, giving a canonical identifier for the subtree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchKey {
    pub height: u8,
    pub node_key: Hash,
}

impl BranchKey {
    /// Compute the parent branch key for a leaf `key` at the given `height`.
    ///
    /// Zeroes bits at big-endian positions ≥ `(256 - height)` in the key,
    /// producing a canonical subtree identifier.
    pub fn new(height: u8, key: &Hash) -> Self {
        let bits_to_zero = height as usize + 1;
        let full_bytes = bits_to_zero / 8;
        let remaining_bits = bits_to_zero % 8;
        let mut bytes = key.as_bytes();
        bytes[size_of::<Hash>() - full_bytes..].fill(0);
        if remaining_bits != 0 {
            bytes[size_of::<Hash>() - full_bytes - 1] &= 0xFF << remaining_bits;
        }
        Self { height, node_key: Hash::from_bytes(bytes) }
    }
}

/// Children of a branch node in the sparse Merkle tree.
///
/// Used both as the in-memory representation and (with the `zerocopy` feature)
/// as the on-disk format for branch version values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "zerocopy",
    derive(zerocopy::FromBytes, zerocopy::IntoBytes, zerocopy::KnownLayout, zerocopy::Immutable, zerocopy::Unaligned)
)]
#[repr(C)]
pub struct BranchChildren {
    pub left: Hash,
    pub right: Hash,
}

/// A single leaf update: set `key` to `leaf_hash` (or remove if `leaf_hash == ZERO_HASH`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeafUpdate {
    pub key: Hash,
    pub leaf_hash: Hash,
}

/// Sorted, unique-by-key collection of leaf updates.
///
/// Required by [`crate::tree::compute_root_update`] which processes leaves bottom-up
/// and relies on sorted order to pair siblings sharing a branch.
///
/// Derefs to `[LeafUpdate]` for read access. No `DerefMut` — mutation could break
/// the sorted/unique invariants.
pub struct SortedLeafUpdates(std::vec::Vec<LeafUpdate>);

impl SortedLeafUpdates {
    /// Build from a sorted map. `BTreeMap` iteration order guarantees
    /// keys are sorted and unique, so no additional sort/dedup is needed.
    pub fn from_sorted_map<V>(map: &BTreeMap<Hash, V>, mut to_leaf_hash: impl FnMut(&Hash, &V) -> Hash) -> Self {
        Self(map.iter().map(|(k, v)| LeafUpdate { key: *k, leaf_hash: to_leaf_hash(k, v) }).collect())
    }

    /// Build from an unsorted iterator. Sorts and deduplicates (last wins).
    pub fn from_unsorted(iter: impl IntoIterator<Item = LeafUpdate>) -> Self {
        let mut v: std::vec::Vec<LeafUpdate> = iter.into_iter().collect();
        v.sort_unstable_by_key(|u| u.key);
        v.dedup_by(|later, earlier| {
            if later.key == earlier.key {
                *earlier = *later;
                true
            } else {
                false
            }
        });
        Self(v)
    }

    pub fn into_vec(self) -> std::vec::Vec<LeafUpdate> {
        self.0
    }
}

impl std::ops::Deref for SortedLeafUpdates {
    type Target = [LeafUpdate];
    fn deref(&self) -> &[LeafUpdate] {
        &self.0
    }
}

/// Read-only branch lookup for the Sparse Merkle Tree.
///
/// This trait is intentionally immutable — tree computations read from the store
/// and return diffs rather than mutating the store. This prevents bugs where
/// unchanged branches are accidentally written.
pub trait SmtStore {
    type Error: core::fmt::Debug + core::fmt::Display;

    fn get_branch(&self, key: &BranchKey) -> Result<Option<BranchChildren>, Self::Error>;
}

/// In-memory SMT store backed by `BTreeMap`s.
///
/// Implements [`SmtStore`] for read-only branch lookups.
/// Mutations (`insert_leaf`, `insert_branch`, etc.) are inherent methods
/// used by [`SparseMerkleTree`](crate::tree::SparseMerkleTree) in tests.
pub struct BTreeSmtStore {
    pub(crate) leaves: BTreeMap<Hash, Hash>,
    pub(crate) branches: BTreeMap<BranchKey, BranchChildren>,
}

impl BTreeSmtStore {
    pub fn new() -> Self {
        Self { leaves: BTreeMap::new(), branches: BTreeMap::new() }
    }

    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    pub fn get_leaf(&self, key: &Hash) -> Option<Hash> {
        self.leaves.get(key).copied()
    }

    pub fn insert_leaf(&mut self, key: Hash, leaf_hash: Hash) {
        if leaf_hash == ZERO_HASH {
            self.leaves.remove(&key);
        } else {
            self.leaves.insert(key, leaf_hash);
        }
    }

    pub fn insert_branch(&mut self, key: BranchKey, children: BranchChildren) {
        self.branches.insert(key, children);
    }
}

impl Default for BTreeSmtStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SmtStore for BTreeSmtStore {
    type Error = Infallible;

    fn get_branch(&self, key: &BranchKey) -> Result<Option<BranchChildren>, Self::Error> {
        Ok(self.branches.get(key).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_key_height_zero() {
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(0, &key);
        assert_eq!(bk.height, 0);
        let mut expected = [0xFF; 32];
        expected[31] = 0xFE;
        assert_eq!(bk.node_key, Hash::from_bytes(expected));
    }

    #[test]
    fn test_branch_key_height_8() {
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(8, &key);
        let mut expected = [0xFF; 32];
        expected[30] = 0xFE;
        expected[31] = 0x00;
        assert_eq!(bk.node_key, Hash::from_bytes(expected));
    }

    #[test]
    fn test_branch_key_height_255() {
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(255, &key);
        assert_eq!(bk.node_key, Hash::from_bytes([0x00; 32]));
    }

    #[test]
    fn test_branch_key_partial_byte() {
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(3, &key);
        let mut expected = [0xFF; 32];
        expected[31] = 0xF0;
        assert_eq!(bk.node_key, Hash::from_bytes(expected));
    }

    #[test]
    fn test_btree_store_leaf_ops() {
        let mut store = BTreeSmtStore::new();
        let key = Hash::from_bytes([1; 32]);
        let val = Hash::from_bytes([2; 32]);

        assert!(store.is_empty());
        assert_eq!(store.get_leaf(&key), None);

        store.insert_leaf(key, val);
        assert!(!store.is_empty());
        assert_eq!(store.get_leaf(&key), Some(val));
        assert_eq!(store.leaf_count(), 1);

        store.insert_leaf(key, ZERO_HASH);
        assert!(store.is_empty());
        assert_eq!(store.get_leaf(&key), None);
    }

    #[test]
    fn test_btree_store_branch_ops() {
        let mut store = BTreeSmtStore::new();
        let bk = BranchKey { height: 5, node_key: ZERO_HASH };
        let left = Hash::from_bytes([1; 32]);
        let right = Hash::from_bytes([2; 32]);

        assert_eq!(store.get_branch(&bk).unwrap(), None);

        store.insert_branch(bk, BranchChildren { left, right });
        assert_eq!(store.get_branch(&bk).unwrap(), Some(BranchChildren { left, right }));
    }
}
