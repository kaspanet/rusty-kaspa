//! Storage trait and in-memory implementation for the Sparse Merkle Tree.
//!
//! [`SmtStore`] abstracts leaf and branch node storage, enabling both
//! in-memory (`BTreeSmtStore`) and persistent (e.g. RocksDB) backends.
//!
//! Branch nodes are keyed by [`BranchKey`] `(height, node_key)` and store
//! the `(left, right)` child hashes. Empty branches (both children equal to
//! the empty subtree hash) are never stored.

use std::collections::BTreeMap;
use std::convert::Infallible;

use kaspa_hashes::Hash;

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
        // Zero the trailing (h+1) big-endian bits of the key to produce
        // a canonical subtree identifier for this branch level.
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

/// Abstraction over leaf and branch node storage for the Sparse Merkle Tree.
///
/// Implementations must support leaf get/insert/remove, branch get/insert/remove,
/// and an emptiness check.
pub trait SmtStore {
    type Error: core::fmt::Debug + core::fmt::Display;

    // Leaf operations
    fn get_leaf(&self, key: &Hash) -> Result<Option<Hash>, Self::Error>;
    fn insert_leaf(&mut self, key: Hash, leaf_hash: Hash) -> Result<(), Self::Error>;
    fn remove_leaf(&mut self, key: &Hash) -> Result<Option<Hash>, Self::Error>;

    // Branch operations
    fn get_branch(&self, key: &BranchKey) -> Result<Option<(Hash, Hash)>, Self::Error>;
    fn insert_branch(&mut self, key: BranchKey, left: Hash, right: Hash) -> Result<(), Self::Error>;
    fn remove_branch(&mut self, key: &BranchKey) -> Result<(), Self::Error>;

    // Queries
    fn is_empty_leaves(&self) -> Result<bool, Self::Error>;
}

/// In-memory [`SmtStore`] backed by `BTreeMap`s.
///
/// Suitable for testing and scenarios where the full tree fits in memory.
/// All operations are infallible.
pub struct BTreeSmtStore {
    leaves: BTreeMap<Hash, Hash>,
    branches: BTreeMap<BranchKey, (Hash, Hash)>,
}

impl BTreeSmtStore {
    pub fn new() -> Self {
        Self { leaves: BTreeMap::new(), branches: BTreeMap::new() }
    }

    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }
}

impl Default for BTreeSmtStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SmtStore for BTreeSmtStore {
    type Error = Infallible;

    fn get_leaf(&self, key: &Hash) -> Result<Option<Hash>, Self::Error> {
        Ok(self.leaves.get(key).copied())
    }

    fn insert_leaf(&mut self, key: Hash, leaf_hash: Hash) -> Result<(), Self::Error> {
        self.leaves.insert(key, leaf_hash);
        Ok(())
    }

    fn remove_leaf(&mut self, key: &Hash) -> Result<Option<Hash>, Self::Error> {
        Ok(self.leaves.remove(key))
    }

    fn get_branch(&self, key: &BranchKey) -> Result<Option<(Hash, Hash)>, Self::Error> {
        Ok(self.branches.get(key).copied())
    }

    fn insert_branch(&mut self, key: BranchKey, left: Hash, right: Hash) -> Result<(), Self::Error> {
        self.branches.insert(key, (left, right));
        Ok(())
    }

    fn remove_branch(&mut self, key: &BranchKey) -> Result<(), Self::Error> {
        self.branches.remove(key);
        Ok(())
    }

    fn is_empty_leaves(&self) -> Result<bool, Self::Error> {
        Ok(self.leaves.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::ZERO_HASH;

    #[test]
    fn test_branch_key_height_zero() {
        // Height 0 = leaf parent (depth 255): zero bit 255 (LSB of byte 31).
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(0, &key);
        assert_eq!(bk.height, 0);
        let mut expected = [0xFF; 32];
        expected[31] = 0xFE;
        assert_eq!(bk.node_key, Hash::from_bytes(expected));
    }

    #[test]
    fn test_branch_key_height_8() {
        // Height 8: zero bits 247..=255 (9 bits: LSB of byte 30 + all of byte 31).
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(8, &key);
        let mut expected = [0xFF; 32];
        expected[30] = 0xFE;
        expected[31] = 0x00;
        assert_eq!(bk.node_key, Hash::from_bytes(expected));
    }

    #[test]
    fn test_branch_key_height_255() {
        // Height 255 (root): zero all bits (positions 0..=255).
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(255, &key);
        assert_eq!(bk.node_key, Hash::from_bytes([0x00; 32]));
    }

    #[test]
    fn test_branch_key_partial_byte() {
        // Height 3: zero positions 252..=255 (bottom 4 bits of byte 31).
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

        assert!(store.is_empty_leaves().unwrap());
        assert_eq!(store.get_leaf(&key).unwrap(), None);

        store.insert_leaf(key, val).unwrap();
        assert!(!store.is_empty_leaves().unwrap());
        assert_eq!(store.get_leaf(&key).unwrap(), Some(val));
        assert_eq!(store.leaf_count(), 1);

        let removed = store.remove_leaf(&key).unwrap();
        assert_eq!(removed, Some(val));
        assert!(store.is_empty_leaves().unwrap());
    }

    #[test]
    fn test_btree_store_branch_ops() {
        let mut store = BTreeSmtStore::new();
        let bk = BranchKey { height: 5, node_key: ZERO_HASH };
        let left = Hash::from_bytes([1; 32]);
        let right = Hash::from_bytes([2; 32]);

        assert_eq!(store.get_branch(&bk).unwrap(), None);

        store.insert_branch(bk, left, right).unwrap();
        assert_eq!(store.get_branch(&bk).unwrap(), Some((left, right)));

        store.remove_branch(&bk).unwrap();
        assert_eq!(store.get_branch(&bk).unwrap(), None);
    }
}
