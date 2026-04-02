//! Storage trait and in-memory implementation for the Sparse Merkle Tree.
//!
//! [`SmtStore`] is a **read-only** trait for node lookups, enabling both
//! in-memory (`BTreeSmtStore`) and persistent (e.g. RocksDB) backends.
//!
//! Nodes are keyed by [`BranchKey`] `(depth, node_key)` and can be
//! either [`Node::Internal`] (node hash) or [`Node::Collapsed`] (single-leaf subtree).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::convert::Infallible;

use kaspa_hashes::{Hash, ZERO_HASH};

/// Key for a branch node in the sparse Merkle tree.
///
/// - `depth`: level from the root. Depth 0 = root node,
///   depth 255 = parent of two leaves.
/// - `node_key`: the leaf key with bits at big-endian positions beyond `depth`
///   zeroed out, giving a canonical identifier for the subtree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchKey {
    pub depth: u8,
    pub node_key: Hash,
}

impl BranchKey {
    /// Compute the branch key for a leaf `key` at the given `depth`.
    ///
    /// Zeroes `(256 - depth)` least-significant bits in the key,
    /// producing a canonical subtree identifier.
    pub fn new(depth: u8, key: &Hash) -> Self {
        let bits_to_zero = 256 - depth as usize;
        let full_bytes = bits_to_zero / 8;
        let remaining_bits = bits_to_zero % 8;
        let mut bytes = key.as_bytes();
        bytes[size_of::<Hash>() - full_bytes..].fill(0);
        if remaining_bits != 0 {
            bytes[size_of::<Hash>() - full_bytes - 1] &= 0xFF << remaining_bits;
        }
        Self { depth, node_key: Hash::from_bytes(bytes) }
    }
}

/// A collapsed single-leaf subtree. Stores the leaf's tree key and precomputed hash.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
    zerocopy::Unaligned,
)]
#[repr(C)]
pub struct CollapsedLeaf {
    pub lane_key: Hash,
    pub leaf_hash: Hash,
}

/// A node in the sparse Merkle tree: either a standard internal branch
/// or a collapsed single-leaf subtree.
///
/// Serialized without a discriminant byte — distinguished by length:
/// - Internal: `hash[32]` = 32 bytes
/// - Collapsed: `lane_key[32] ++ leaf_hash[32]` = 64 bytes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    /// Standard internal node — stores the node's own hash.
    Internal(Hash),
    /// Collapsed single-leaf subtree.
    Collapsed(CollapsedLeaf),
}

impl Node {
    /// Serialize to bytes. Length-discriminated: 32B for Internal, 64B for Collapsed.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Node::Internal(hash) => hash.as_bytes().to_vec(),
            Node::Collapsed(cl) => {
                let mut v = Vec::with_capacity(64);
                v.extend_from_slice(&cl.lane_key.as_bytes());
                v.extend_from_slice(&cl.leaf_hash.as_bytes());
                v
            }
        }
    }

    /// Deserialize from bytes. 32B → Internal, 64B → Collapsed.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes.len() {
            32 => Some(Node::Internal(Hash::from_bytes(bytes.try_into().unwrap()))),
            64 => {
                let lane_key = Hash::from_bytes(bytes[..32].try_into().unwrap());
                let leaf_hash = Hash::from_bytes(bytes[32..].try_into().unwrap());
                Some(Node::Collapsed(CollapsedLeaf { lane_key, leaf_hash }))
            }
            _ => None,
        }
    }
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

/// Borrowed view over sorted, unique-by-key leaf updates.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct SortedLeafUpdatesRef<'a>(&'a [LeafUpdate]);

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

    pub fn as_ref(&self) -> SortedLeafUpdatesRef<'_> {
        SortedLeafUpdatesRef(&self.0)
    }

    pub(crate) fn from_sorted_vec(v: std::vec::Vec<LeafUpdate>) -> Self {
        Self(v)
    }

    pub fn single(&self) -> Option<&LeafUpdate> {
        match self.0.as_slice() {
            [u] => Some(u),
            _ => None,
        }
    }

    pub fn contains_key(&self, key: &Hash) -> bool {
        self.0.binary_search_by_key(key, |u| u.key).is_ok()
    }

    pub fn partition_by_bit(&self, depth: usize) -> (SortedLeafUpdatesRef<'_>, SortedLeafUpdatesRef<'_>) {
        let split_pos = self.0.partition_point(|u| !crate::bit_at(&u.key, depth));
        (SortedLeafUpdatesRef(&self.0[..split_pos]), SortedLeafUpdatesRef(&self.0[split_pos..]))
    }

    pub fn first_with_bit(&self, depth: usize, right: bool) -> Option<&LeafUpdate> {
        let (left, right_slice) = self.partition_by_bit(depth);
        if right { right_slice.first() } else { left.first() }
    }
}

impl std::ops::Deref for SortedLeafUpdates {
    type Target = [LeafUpdate];
    fn deref(&self) -> &[LeafUpdate] {
        &self.0
    }
}

impl<'a> SortedLeafUpdatesRef<'a> {
    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    pub fn len(self) -> usize {
        self.0.len()
    }

    pub fn first(self) -> Option<&'a LeafUpdate> {
        self.0.first()
    }

    pub fn as_slice(self) -> &'a [LeafUpdate] {
        self.0
    }

    pub fn single(self) -> Option<&'a LeafUpdate> {
        match self.0 {
            [u] => Some(u),
            _ => None,
        }
    }

    pub fn binary_search_by_key(self, key: &Hash) -> Result<usize, usize> {
        self.0.binary_search_by_key(key, |u| u.key)
    }

    pub fn partition_by_bit(self, depth: usize) -> (Self, Self) {
        let split_pos = self.0.partition_point(|u| !crate::bit_at(&u.key, depth));
        (Self(&self.0[..split_pos]), Self(&self.0[split_pos..]))
    }

    pub fn first_with_bit(self, depth: usize, right: bool) -> Option<&'a LeafUpdate> {
        let (left, right_slice) = self.partition_by_bit(depth);
        if right { right_slice.first() } else { left.first() }
    }
}

/// Read-only node lookup for the Sparse Merkle Tree.
///
/// This trait is intentionally immutable — tree computations read from the store
/// and return diffs rather than mutating the store. This prevents bugs where
/// unchanged nodes are accidentally written.
pub trait SmtStore {
    type Error: core::fmt::Debug + core::fmt::Display;

    /// Read a node (internal or collapsed) from the store.
    fn get_node(&self, key: &BranchKey) -> Result<Option<Node>, Self::Error>;

    /// Read a leaf hash by its key. Used by `prove()` at the leaf-parent level
    /// to get sibling leaf hashes that aren't stored as branch nodes.
    /// Default returns `None` (suitable for consensus stores that don't need prove).
    fn get_leaf(&self, _key: &Hash) -> Result<Option<Hash>, Self::Error> {
        Ok(None)
    }
}

/// In-memory SMT store backed by `BTreeMap`s.
///
/// Implements [`SmtStore`] for read-only node lookups.
/// Mutations (`insert_leaf`, `insert_node`) are inherent methods
/// used by [`SparseMerkleTree`](crate::tree::SparseMerkleTree) in tests.
pub struct BTreeSmtStore {
    pub(crate) leaves: BTreeMap<Hash, Hash>,
    pub(crate) nodes: BTreeMap<BranchKey, Node>,
}

impl BTreeSmtStore {
    pub fn new() -> Self {
        Self { leaves: BTreeMap::new(), nodes: BTreeMap::new() }
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

    pub fn insert_node(&mut self, key: BranchKey, node: Option<Node>) {
        match node {
            Some(node) => {
                self.nodes.insert(key, node);
            }
            None => {
                self.nodes.remove(&key);
            }
        }
    }
}

impl Default for BTreeSmtStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SmtStore for BTreeSmtStore {
    type Error = Infallible;

    fn get_node(&self, key: &BranchKey) -> Result<Option<Node>, Self::Error> {
        Ok(self.nodes.get(key).copied())
    }

    fn get_leaf(&self, key: &Hash) -> Result<Option<Hash>, Self::Error> {
        Ok(self.leaves.get(key).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_key_depth_255() {
        // Leaf-parent: depth 255 → zeroes 1 bit (LSB)
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(255, &key);
        assert_eq!(bk.depth, 255);
        let mut expected = [0xFF; 32];
        expected[31] = 0xFE;
        assert_eq!(bk.node_key, Hash::from_bytes(expected));
    }

    #[test]
    fn test_branch_key_depth_247() {
        // depth 247 → zeroes 9 bits
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(247, &key);
        let mut expected = [0xFF; 32];
        expected[30] = 0xFE;
        expected[31] = 0x00;
        assert_eq!(bk.node_key, Hash::from_bytes(expected));
    }

    #[test]
    fn test_branch_key_depth_0() {
        // Root: depth 0 → zeroes all 256 bits
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(0, &key);
        assert_eq!(bk.node_key, Hash::from_bytes([0x00; 32]));
    }

    #[test]
    fn test_branch_key_partial_byte() {
        // depth 252 → zeroes 4 bits
        let key = Hash::from_bytes([0xFF; 32]);
        let bk = BranchKey::new(252, &key);
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
    fn test_btree_store_node_ops() {
        let mut store = BTreeSmtStore::new();
        let bk = BranchKey { depth: 5, node_key: ZERO_HASH };
        let hash = Hash::from_bytes([1; 32]);

        assert_eq!(store.get_node(&bk).unwrap(), None);

        store.insert_node(bk, Some(Node::Internal(hash)));
        assert_eq!(store.get_node(&bk).unwrap(), Some(Node::Internal(hash)));
    }

    #[test]
    fn test_node_serialization_internal() {
        let hash = Hash::from_bytes([0x42; 32]);
        let node = Node::Internal(hash);
        let bytes = node.to_bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(Node::from_bytes(&bytes), Some(node));
    }

    #[test]
    fn test_node_serialization_collapsed() {
        let cl = CollapsedLeaf { lane_key: Hash::from_bytes([0x11; 32]), leaf_hash: Hash::from_bytes([0x22; 32]) };
        let node = Node::Collapsed(cl);
        let bytes = node.to_bytes();
        assert_eq!(bytes.len(), 64);
        assert_eq!(Node::from_bytes(&bytes), Some(node));
    }
}
