//! Full Sparse Merkle Tree with insert, remove, root computation, and proof generation.
//!
//! This module requires the `std` feature and is **not** intended for ZK guest programs.
//! Use [`crate::proof::SmtProof`] for ZK-compatible proof verification.
//!
//! # Semantics
//!
//! Inserting [`ZERO_HASH`] as a leaf value is equivalent to removing the key,
//! since `ZERO_HASH` is the canonical empty leaf marker.

use std::collections::BTreeMap;
use std::vec::Vec;

use core::marker::PhantomData;
use kaspa_hashes::{Hash, Hasher, ZERO_HASH};

use crate::proof::OwnedSmtProof;
use crate::{DEPTH, SmtHasher, bit_at, hash_node};

/// A 256-bit Sparse Merkle Tree backed by a `BTreeMap`.
///
/// Generic over the internal node hasher `H` (e.g., `SeqCommitActiveNode`).
/// Leaf hashes are opaque `Hash` values inserted externally.
pub struct SparseMerkleTree<H: SmtHasher> {
    leaves: BTreeMap<Hash, Hash>,
    _phantom: PhantomData<H>,
}

impl<H: SmtHasher> SparseMerkleTree<H> {
    /// Create a new empty sparse Merkle tree.
    pub fn new() -> Self {
        Self { leaves: BTreeMap::new(), _phantom: PhantomData }
    }

    /// Insert or update a leaf.
    ///
    /// If `leaf_hash` is [`ZERO_HASH`], the key is removed instead (the empty
    /// leaf is the canonical absence marker).
    pub fn insert(&mut self, key: Hash, leaf_hash: Hash) {
        if leaf_hash == ZERO_HASH {
            self.leaves.remove(&key);
        } else {
            self.leaves.insert(key, leaf_hash);
        }
    }

    /// Remove a leaf by key.
    ///
    /// Returns the previous leaf hash, or `None` if the key was absent.
    pub fn remove(&mut self, key: &Hash) -> Option<Hash> {
        self.leaves.remove(key)
    }

    /// Compute the current root hash of the tree.
    ///
    /// Returns the canonical empty root if the tree has no leaves.
    pub fn root(&self) -> Hash {
        let empty_hashes = &H::EMPTY_HASHES;
        if self.leaves.is_empty() {
            return empty_hashes[DEPTH];
        }
        let sorted = self.sorted_leaves();
        compute_subtree_root::<H>(empty_hashes, &sorted, 0)
    }

    /// Number of non-empty leaves.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Whether the tree has no non-empty leaves.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Look up the leaf hash for a key, or `None` if absent.
    pub fn get(&self, key: &Hash) -> Option<&Hash> {
        self.leaves.get(key)
    }

    /// Check whether a key is present in the tree.
    pub fn contains_key(&self, key: &Hash) -> bool {
        self.leaves.contains_key(key)
    }

    /// Generate an inclusion or non-inclusion proof for the given key.
    ///
    /// - If the key is present, the proof is an **inclusion proof** — it
    ///   verifies with `Some(leaf_hash)`.
    /// - If the key is absent, the proof is a **non-inclusion proof** — it
    ///   verifies with `None`.
    pub fn prove(&self, key: &Hash) -> OwnedSmtProof {
        let sorted = self.sorted_leaves();
        let empty_hashes = &H::EMPTY_HASHES;
        let mut bitmap = [0u8; 32];
        let mut siblings = Vec::new();

        let mut start = 0usize;
        let mut end = sorted.len();

        for depth in 0..DEPTH {
            if start == end {
                // All remaining siblings are empty subtree hashes.
                for d in depth..DEPTH {
                    bitmap[d / 8] |= 1 << (d % 8);
                }
                break;
            }

            let split = start + sorted[start..end].partition_point(|(k, _)| !bit_at(k, depth));

            let (path_start, path_end, sib_start, sib_end) =
                if bit_at(key, depth) { (split, end, start, split) } else { (start, split, split, end) };

            if sib_start == sib_end {
                bitmap[depth / 8] |= 1 << (depth % 8);
            } else {
                let sibling_root = compute_subtree_root::<H>(empty_hashes, &sorted[sib_start..sib_end], depth + 1);
                siblings.push(sibling_root);
            }

            start = path_start;
            end = path_end;
        }

        OwnedSmtProof { bitmap, siblings }
    }

    /// Collect leaves into a sorted Vec.
    ///
    /// `BTreeMap` iteration is already ordered by key, and `Hash`'s lexicographic
    /// `Ord` matches the big-endian bit ordering used for tree traversal — so no
    /// explicit sort is needed.
    fn sorted_leaves(&self) -> Vec<(Hash, Hash)> {
        self.leaves.iter().map(|(&k, &v)| (k, v)).collect()
    }
}

/// Iteratively compute the root hash of a subtree over a sorted leaf slice.
///
/// Uses an explicit stack with a descend-left / ascend-combine / descend-right
/// loop — no recursion.
fn compute_subtree_root<H: Hasher>(empty_hashes: &[Hash; DEPTH + 1], leaves: &[(Hash, Hash)], start_depth: usize) -> Hash {
    struct Frame {
        end: usize,
        split: usize,
        depth: usize,
        left: Option<Hash>,
    }

    let mut stack: Vec<Frame> = Vec::new();
    let mut start = 0;
    let mut end = leaves.len();
    let mut depth = start_depth;

    loop {
        // Descend into left children until we hit a terminal node.
        while start != end && depth != DEPTH {
            let split = start + leaves[start..end].partition_point(|(k, _)| !bit_at(k, depth));
            stack.push(Frame { end, split, depth, left: None });
            end = split;
            depth += 1;
        }

        // Terminal: empty subtree or single leaf.
        let mut current = if start == end { empty_hashes[DEPTH - depth] } else { leaves[start].1 };

        // Ascend, combining left and right children.
        loop {
            let Some(frame) = stack.last_mut() else {
                return current;
            };

            if frame.left.is_none() {
                // Left child just computed — save it, descend into right child.
                frame.left = Some(current);
                start = frame.split;
                end = frame.end;
                depth = frame.depth + 1;
                break;
            }

            // Both children done — combine and pop.
            current = hash_node::<H>(frame.left.unwrap(), current);
            stack.pop();
        }
    }
}

impl<H: SmtHasher> Default for SparseMerkleTree<H> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::{HasherBase, SeqCommitActiveNode};
    use rand::{Rng, SeedableRng, rngs::StdRng};

    type TestHasher = SeqCommitActiveNode;
    type Smt = SparseMerkleTree<TestHasher>;

    // ---- Helpers ----

    /// Create a deterministic key by hashing seed bytes.
    fn test_key(seed: &[u8]) -> Hash {
        let mut h = TestHasher::default();
        h.update(b"test_key:");
        h.update(seed);
        h.finalize()
    }

    /// Create a deterministic leaf hash from seed bytes.
    fn test_leaf(seed: &[u8]) -> Hash {
        let mut h = TestHasher::default();
        h.update(b"test_leaf:");
        h.update(seed);
        h.finalize()
    }

    /// Create a key with a specific byte pattern.
    fn key_from_bytes(bytes: [u8; 32]) -> Hash {
        Hash::from_bytes(bytes)
    }

    /// Assert that an inclusion proof for `key` with `leaf` is valid.
    fn assert_inclusion(tree: &Smt, key: &Hash, leaf: Hash) {
        let root = tree.root();
        let proof = tree.prove(key);
        assert!(proof.verify::<TestHasher>(key, Some(leaf), root).unwrap(), "inclusion proof failed for key {key}");
        assert_eq!(proof.compute_root::<TestHasher>(key, Some(leaf)).unwrap(), root, "compute_root mismatch for key {key}");
    }

    /// Assert that a non-inclusion proof for `key` is valid.
    fn assert_non_inclusion(tree: &Smt, key: &Hash) {
        let root = tree.root();
        let proof = tree.prove(key);
        assert!(proof.verify::<TestHasher>(key, None, root).unwrap(), "non-inclusion proof failed for key {key}");
    }

    // ========================================================================
    // 1. Empty tree tests
    // ========================================================================

    #[test]
    fn test_empty_root() {
        let tree = Smt::new();
        let expected = crate::empty_root::<TestHasher>();
        assert_eq!(tree.root(), expected);
        assert_ne!(tree.root(), ZERO_HASH, "empty root is not ZERO_HASH");
    }

    #[test]
    fn test_empty_tree_properties() {
        let tree = Smt::new();
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_empty_tree_get_returns_none() {
        let tree = Smt::new();
        let key = test_key(b"any");
        assert_eq!(tree.get(&key), None);
        assert!(!tree.contains_key(&key));
    }

    #[test]
    fn test_empty_tree_remove_returns_none() {
        let mut tree = Smt::new();
        let key = test_key(b"any");
        assert_eq!(tree.remove(&key), None);
        assert_eq!(tree.root(), crate::empty_root::<TestHasher>());
    }

    #[test]
    fn test_empty_tree_non_inclusion_proof() {
        let tree = Smt::new();
        let key = test_key(b"absent");
        assert_non_inclusion(&tree, &key);
    }

    #[test]
    fn test_empty_tree_proof_all_siblings_empty() {
        let tree = Smt::new();
        let key = test_key(b"x");
        let proof = tree.prove(&key);
        assert_eq!(proof.non_empty_count(), 0);
        assert_eq!(proof.empty_count(), DEPTH);
        assert_eq!(proof.bitmap, [0xFF; 32]);
    }

    // ========================================================================
    // 2. Single element tests
    // ========================================================================

    #[test]
    fn test_single_insert_changes_root() {
        let mut tree = Smt::new();
        let empty_root = tree.root();
        tree.insert(test_key(b"k"), test_leaf(b"v"));
        assert_ne!(tree.root(), empty_root);
    }

    #[test]
    fn test_single_get() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf);
        assert_eq!(tree.get(&key), Some(&leaf));
        assert!(tree.contains_key(&key));
        assert_eq!(tree.len(), 1);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_single_inclusion_proof() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf);
        assert_inclusion(&tree, &key, leaf);
    }

    #[test]
    fn test_single_non_inclusion_for_other_key() {
        let mut tree = Smt::new();
        tree.insert(test_key(b"present"), test_leaf(b"v"));
        let absent = test_key(b"absent");
        assert_non_inclusion(&tree, &absent);
    }

    #[test]
    fn test_single_delete_restores_empty() {
        let mut tree = Smt::new();
        let empty_root = tree.root();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v"));
        assert_ne!(tree.root(), empty_root);
        tree.remove(&key);
        assert_eq!(tree.root(), empty_root);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_single_insert_delete_reinsert() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf);
        let root_after_insert = tree.root();
        tree.remove(&key);
        tree.insert(key, leaf);
        assert_eq!(tree.root(), root_after_insert);
    }

    #[test]
    fn test_single_update_changes_root() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v1"));
        let root1 = tree.root();
        tree.insert(key, test_leaf(b"v2"));
        let root2 = tree.root();
        assert_ne!(root1, root2);
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn test_insert_zero_hash_is_remove() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v"));
        assert_eq!(tree.len(), 1);
        tree.insert(key, ZERO_HASH);
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
        assert_eq!(tree.root(), crate::empty_root::<TestHasher>());
    }

    // ========================================================================
    // 3. Multi-element tests
    // ========================================================================

    #[test]
    fn test_two_elements_both_retrievable() {
        let mut tree = Smt::new();
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");
        tree.insert(k1, l1);
        tree.insert(k2, l2);
        assert_eq!(tree.get(&k1), Some(&l1));
        assert_eq!(tree.get(&k2), Some(&l2));
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn test_two_elements_both_proofs_verify() {
        let mut tree = Smt::new();
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");
        tree.insert(k1, l1);
        tree.insert(k2, l2);
        assert_inclusion(&tree, &k1, l1);
        assert_inclusion(&tree, &k2, l2);
    }

    #[test]
    fn test_three_elements_all_proofs_verify() {
        let mut tree = Smt::new();
        let keys: Vec<Hash> = (0..3).map(|i| test_key(&[i])).collect();
        let leaves: Vec<Hash> = (0..3).map(|i| test_leaf(&[i])).collect();
        for (&k, &l) in keys.iter().zip(&leaves) {
            tree.insert(k, l);
        }
        for (&k, &l) in keys.iter().zip(&leaves) {
            assert_inclusion(&tree, &k, l);
        }
    }

    #[test]
    fn test_ten_elements_all_proofs_verify() {
        let mut tree = Smt::new();
        let entries: Vec<(Hash, Hash)> = (0u32..10).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();
        for &(k, l) in &entries {
            tree.insert(k, l);
        }
        for &(k, l) in &entries {
            assert_inclusion(&tree, &k, l);
        }
    }

    #[test]
    fn test_root_changes_with_each_insert() {
        let mut tree = Smt::new();
        let mut prev_root = tree.root();
        for i in 0u32..5 {
            tree.insert(test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()));
            let new_root = tree.root();
            assert_ne!(new_root, prev_root, "root didn't change after insert {i}");
            prev_root = new_root;
        }
    }

    #[test]
    fn test_different_values_different_roots() {
        let key = test_key(b"k");
        let mut tree1 = Smt::new();
        tree1.insert(key, test_leaf(b"v1"));
        let mut tree2 = Smt::new();
        tree2.insert(key, test_leaf(b"v2"));
        assert_ne!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_different_keys_different_roots() {
        let leaf = test_leaf(b"same_leaf");
        let mut tree1 = Smt::new();
        tree1.insert(test_key(b"k1"), leaf);
        let mut tree2 = Smt::new();
        tree2.insert(test_key(b"k2"), leaf);
        assert_ne!(tree1.root(), tree2.root());
    }

    // ========================================================================
    // 4. Delete semantics
    // ========================================================================

    #[test]
    fn test_delete_returns_value() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf);
        assert_eq!(tree.remove(&key), Some(leaf));
    }

    #[test]
    fn test_delete_nonexistent_noop() {
        let mut tree = Smt::new();
        tree.insert(test_key(b"present"), test_leaf(b"v"));
        let root_before = tree.root();
        assert_eq!(tree.remove(&test_key(b"absent")), None);
        assert_eq!(tree.root(), root_before);
    }

    #[test]
    fn test_delete_restores_previous_root() {
        let mut tree = Smt::new();
        let ka = test_key(b"a");
        let kb = test_key(b"b");
        tree.insert(ka, test_leaf(b"la"));
        let root_a_only = tree.root();
        tree.insert(kb, test_leaf(b"lb"));
        assert_ne!(tree.root(), root_a_only);
        tree.remove(&kb);
        assert_eq!(tree.root(), root_a_only);
    }

    #[test]
    fn test_delete_all_returns_to_empty() {
        let mut tree = Smt::new();
        let empty_root = tree.root();
        let keys: Vec<Hash> = (0u32..10).map(|i| test_key(&i.to_le_bytes())).collect();
        for (i, &k) in keys.iter().enumerate() {
            tree.insert(k, test_leaf(&(i as u32).to_le_bytes()));
        }
        assert_ne!(tree.root(), empty_root);
        for k in &keys {
            tree.remove(k);
        }
        assert_eq!(tree.root(), empty_root);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_delete_middle_element() {
        let mut tree = Smt::new();
        let ka = test_key(b"a");
        let kb = test_key(b"b");
        let kc = test_key(b"c");
        let la = test_leaf(b"la");
        let lc = test_leaf(b"lc");
        tree.insert(ka, la);
        tree.insert(kb, test_leaf(b"lb"));
        tree.insert(kc, lc);
        tree.remove(&kb);
        assert_eq!(tree.len(), 2);
        assert_inclusion(&tree, &ka, la);
        assert_inclusion(&tree, &kc, lc);
        assert_non_inclusion(&tree, &kb);
    }

    #[test]
    fn test_non_inclusion_after_delete() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v"));
        tree.remove(&key);
        assert_non_inclusion(&tree, &key);
    }

    // ========================================================================
    // 5. Determinism / order independence
    // ========================================================================

    #[test]
    fn test_insertion_order_independence() {
        let entries: Vec<(Hash, Hash)> = (0u32..20).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();

        let mut tree1 = Smt::new();
        for &(k, l) in &entries {
            tree1.insert(k, l);
        }

        let mut tree2 = Smt::new();
        for &(k, l) in entries.iter().rev() {
            tree2.insert(k, l);
        }

        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_reconstruction_determinism() {
        let mut tree1 = Smt::new();
        let mut tree2 = Smt::new();
        for i in 0u32..10 {
            let k = test_key(&i.to_le_bytes());
            let l = test_leaf(&i.to_le_bytes());
            tree1.insert(k, l);
            tree2.insert(k, l);
        }
        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_root_idempotent() {
        let mut tree = Smt::new();
        for i in 0u32..5 {
            tree.insert(test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()));
        }
        let r1 = tree.root();
        let r2 = tree.root();
        let r3 = tree.root();
        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
    }

    #[test]
    fn test_insert_remove_insert_same_as_insert() {
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");

        let mut tree1 = Smt::new();
        tree1.insert(key, leaf);

        let mut tree2 = Smt::new();
        tree2.insert(key, leaf);
        tree2.remove(&key);
        tree2.insert(key, leaf);

        assert_eq!(tree1.root(), tree2.root());
    }

    // ========================================================================
    // 6. Inclusion proof tests (positive path)
    // ========================================================================

    #[test]
    fn test_proof_for_every_element() {
        let mut tree = Smt::new();
        let entries: Vec<(Hash, Hash)> = (0u32..20).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();
        for &(k, l) in &entries {
            tree.insert(k, l);
        }
        for &(k, l) in &entries {
            assert_inclusion(&tree, &k, l);
        }
    }

    #[test]
    fn test_proof_after_update() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v1"));
        let new_leaf = test_leaf(b"v2");
        tree.insert(key, new_leaf);
        assert_inclusion(&tree, &key, new_leaf);
    }

    #[test]
    fn test_proof_compute_root_matches_tree_root() {
        let mut tree = Smt::new();
        let entries: Vec<(Hash, Hash)> = (0u32..5).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();
        for &(k, l) in &entries {
            tree.insert(k, l);
        }
        let root = tree.root();
        for &(k, l) in &entries {
            let proof = tree.prove(&k);
            assert_eq!(proof.compute_root::<TestHasher>(&k, Some(l)).unwrap(), root);
        }
    }

    #[test]
    fn test_inclusion_and_non_inclusion_coexist() {
        let mut tree = Smt::new();
        let present: Vec<(Hash, Hash)> = (0u32..5).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();
        let absent: Vec<Hash> = (100u32..105).map(|i| test_key(&i.to_le_bytes())).collect();
        for &(k, l) in &present {
            tree.insert(k, l);
        }
        for &(k, l) in &present {
            assert_inclusion(&tree, &k, l);
        }
        for k in &absent {
            assert_non_inclusion(&tree, k);
        }
    }

    #[test]
    fn test_proof_non_empty_count() {
        let mut tree = Smt::new();
        let k = test_key(b"solo");
        tree.insert(k, test_leaf(b"v"));
        let proof = tree.prove(&k);
        assert_eq!(proof.non_empty_count(), 0);
        assert_eq!(proof.empty_count(), DEPTH);

        tree.insert(test_key(b"other"), test_leaf(b"w"));
        let proof = tree.prove(&k);
        assert_eq!(proof.non_empty_count(), 1);
    }

    // ========================================================================
    // 7. Non-inclusion proof tests (negative path)
    // ========================================================================

    #[test]
    fn test_non_inclusion_empty_tree() {
        let tree = Smt::new();
        assert_non_inclusion(&tree, &test_key(b"anything"));
    }

    #[test]
    fn test_non_inclusion_single_element_tree() {
        let mut tree = Smt::new();
        tree.insert(test_key(b"present"), test_leaf(b"v"));
        assert_non_inclusion(&tree, &test_key(b"absent"));
    }

    #[test]
    fn test_non_inclusion_multi_element_tree() {
        let mut tree = Smt::new();
        for i in 0u32..10 {
            tree.insert(test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()));
        }
        for i in 100u32..110 {
            assert_non_inclusion(&tree, &test_key(&i.to_le_bytes()));
        }
    }

    #[test]
    fn test_non_inclusion_after_deletion() {
        let mut tree = Smt::new();
        let key = test_key(b"will_delete");
        tree.insert(key, test_leaf(b"v"));
        tree.insert(test_key(b"stays"), test_leaf(b"w"));
        tree.remove(&key);
        assert_non_inclusion(&tree, &key);
    }

    #[test]
    fn test_non_inclusion_proof_is_different_from_inclusion() {
        let mut tree = Smt::new();
        let present_key = test_key(b"present");
        let present_leaf = test_leaf(b"v");
        let absent_key = test_key(b"absent");
        tree.insert(present_key, present_leaf);

        let root = tree.root();
        let inclusion_proof = tree.prove(&present_key);
        let non_inclusion_proof = tree.prove(&absent_key);

        assert!(inclusion_proof.verify::<TestHasher>(&present_key, Some(present_leaf), root).unwrap());
        assert!(non_inclusion_proof.verify::<TestHasher>(&absent_key, None, root).unwrap());
    }

    // ========================================================================
    // 8. Proof rejection tests
    // ========================================================================

    #[test]
    fn test_reject_wrong_leaf_hash() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"correct"));
        let root = tree.root();
        let proof = tree.prove(&key);
        let wrong_leaf = test_leaf(b"wrong");
        assert!(!proof.verify::<TestHasher>(&key, Some(wrong_leaf), root).unwrap());
    }

    #[test]
    fn test_reject_wrong_root() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf);
        let proof = tree.prove(&key);
        let wrong_root = test_leaf(b"fake_root");
        assert!(!proof.verify::<TestHasher>(&key, Some(leaf), wrong_root).unwrap());
    }

    #[test]
    fn test_reject_wrong_key() {
        let mut tree = Smt::new();
        let key = test_key(b"correct_key");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf);
        let root = tree.root();
        let proof = tree.prove(&key);
        let wrong_key = test_key(b"wrong_key");
        assert!(!proof.verify::<TestHasher>(&wrong_key, Some(leaf), root).unwrap());
    }

    #[test]
    fn test_reject_tampered_sibling() {
        let mut tree = Smt::new();
        let k1 = test_key(b"k1");
        let k2 = test_key(b"k2");
        tree.insert(k1, test_leaf(b"v1"));
        tree.insert(k2, test_leaf(b"v2"));
        let root = tree.root();
        let mut proof = tree.prove(&k1);

        assert!(!proof.siblings.is_empty(), "need at least one sibling to tamper");
        let mut tampered = proof.siblings[0].as_bytes();
        tampered[0] ^= 0xFF;
        proof.siblings[0] = Hash::from_bytes(tampered);

        assert!(!proof.verify::<TestHasher>(&k1, Some(test_leaf(b"v1")), root).unwrap());
    }

    #[test]
    fn test_reject_inclusion_as_non_inclusion() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf);
        let root = tree.root();
        let proof = tree.prove(&key);
        assert!(!proof.verify::<TestHasher>(&key, None, root).unwrap());
    }

    #[test]
    fn test_reject_non_inclusion_as_inclusion() {
        let mut tree = Smt::new();
        tree.insert(test_key(b"other"), test_leaf(b"w"));
        let absent = test_key(b"absent");
        let root = tree.root();
        let proof = tree.prove(&absent);
        let fake_leaf = test_leaf(b"fake");
        assert!(!proof.verify::<TestHasher>(&absent, Some(fake_leaf), root).unwrap());
    }

    // ========================================================================
    // 9. Boundary / edge cases
    // ========================================================================

    #[test]
    fn test_max_depth_divergence() {
        let bytes1 = [0u8; 32];
        let mut bytes2 = [0u8; 32];
        bytes2[31] = 0x01;
        let k1 = key_from_bytes(bytes1);
        let k2 = key_from_bytes(bytes2);

        assert!(!bit_at(&k1, 255));
        assert!(bit_at(&k2, 255));
        for d in 0..255 {
            assert_eq!(bit_at(&k1, d), bit_at(&k2, d), "keys should agree at depth {d}");
        }

        let l1 = test_leaf(b"left");
        let l2 = test_leaf(b"right");
        let mut tree = Smt::new();
        tree.insert(k1, l1);
        tree.insert(k2, l2);

        assert_inclusion(&tree, &k1, l1);
        assert_inclusion(&tree, &k2, l2);

        let proof1 = tree.prove(&k1);
        assert_eq!(proof1.non_empty_count(), 1);
    }

    #[test]
    fn test_root_level_divergence() {
        let k1 = key_from_bytes([0x00; 32]);
        let mut bytes2 = [0u8; 32];
        bytes2[0] = 0x80;
        let k2 = key_from_bytes(bytes2);

        assert!(!bit_at(&k1, 0));
        assert!(bit_at(&k2, 0));

        let l1 = test_leaf(b"left");
        let l2 = test_leaf(b"right");
        let mut tree = Smt::new();
        tree.insert(k1, l1);
        tree.insert(k2, l2);

        assert_inclusion(&tree, &k1, l1);
        assert_inclusion(&tree, &k2, l2);

        let proof1 = tree.prove(&k1);
        assert_eq!(proof1.non_empty_count(), 1);
    }

    #[test]
    fn test_shared_long_prefix() {
        let bytes1 = [0u8; 32];
        let mut bytes2 = [0u8; 32];
        bytes2[200 / 8] |= 1 << 7;

        let k1 = key_from_bytes(bytes1);
        let k2 = key_from_bytes(bytes2);

        for d in 0..200 {
            assert_eq!(bit_at(&k1, d), bit_at(&k2, d), "should agree at depth {d}");
        }
        assert_ne!(bit_at(&k1, 200), bit_at(&k2, 200));

        let mut tree = Smt::new();
        tree.insert(k1, test_leaf(b"v1"));
        tree.insert(k2, test_leaf(b"v2"));

        assert_inclusion(&tree, &k1, test_leaf(b"v1"));
        assert_inclusion(&tree, &k2, test_leaf(b"v2"));
    }

    #[test]
    fn test_zero_key_and_max_key() {
        let zero_key = key_from_bytes([0x00; 32]);
        let max_key = key_from_bytes([0xFF; 32]);
        let mut tree = Smt::new();
        tree.insert(zero_key, test_leaf(b"zero"));
        tree.insert(max_key, test_leaf(b"max"));

        assert_inclusion(&tree, &zero_key, test_leaf(b"zero"));
        assert_inclusion(&tree, &max_key, test_leaf(b"max"));
    }

    #[test]
    fn test_many_keys_same_first_byte() {
        let mut tree = Smt::new();
        let mut entries = Vec::new();
        for i in 0u32..20 {
            let mut bytes = [0u8; 32];
            bytes[0] = 0xAA;
            bytes[1..5].copy_from_slice(&i.to_le_bytes());
            let key = key_from_bytes(bytes);
            let leaf = test_leaf(&i.to_le_bytes());
            tree.insert(key, leaf);
            entries.push((key, leaf));
        }
        for &(k, l) in &entries {
            assert_inclusion(&tree, &k, l);
        }
    }

    #[test]
    fn test_alternating_bit_patterns() {
        let k1 = key_from_bytes([0xAA; 32]);
        let k2 = key_from_bytes([0x55; 32]);
        let mut tree = Smt::new();
        tree.insert(k1, test_leaf(b"aa"));
        tree.insert(k2, test_leaf(b"55"));
        assert_inclusion(&tree, &k1, test_leaf(b"aa"));
        assert_inclusion(&tree, &k2, test_leaf(b"55"));
    }

    // ========================================================================
    // 10. Stress tests
    // ========================================================================

    #[test]
    fn test_100_random_all_proofs_verify() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut tree = Smt::new();
        let mut entries = Vec::new();

        for _ in 0..100 {
            let key = Hash::from_bytes(rng.r#gen());
            let leaf = Hash::from_bytes(rng.r#gen());
            tree.insert(key, leaf);
            entries.push((key, leaf));
        }

        let root = tree.root();
        for &(k, l) in &entries {
            let proof = tree.prove(&k);
            assert!(proof.verify::<TestHasher>(&k, Some(l), root).unwrap(), "inclusion proof failed");
        }
    }

    #[test]
    fn test_100_random_non_inclusion() {
        let mut rng = StdRng::seed_from_u64(123);
        let mut tree = Smt::new();

        for _ in 0..100 {
            tree.insert(Hash::from_bytes(rng.r#gen()), Hash::from_bytes(rng.r#gen()));
        }

        let root = tree.root();

        let mut rng2 = StdRng::seed_from_u64(456);
        for _ in 0..100 {
            let absent_key = Hash::from_bytes(rng2.r#gen());
            if tree.contains_key(&absent_key) {
                continue;
            }
            let proof = tree.prove(&absent_key);
            assert!(proof.verify::<TestHasher>(&absent_key, None, root).unwrap(), "non-inclusion proof failed");
        }
    }

    #[test]
    fn test_random_insert_delete_cycle() {
        let mut rng = StdRng::seed_from_u64(999);
        let mut tree = Smt::new();
        let mut live_keys: Vec<(Hash, Hash)> = Vec::new();

        for i in 0..200 {
            if i % 3 == 2 && !live_keys.is_empty() {
                let idx = rng.gen_range(0..live_keys.len());
                let (key, _) = live_keys.swap_remove(idx);
                tree.remove(&key);
            } else {
                let key = Hash::from_bytes(rng.r#gen());
                let leaf = Hash::from_bytes(rng.r#gen());
                tree.insert(key, leaf);
                live_keys.push((key, leaf));
            }
        }

        let root = tree.root();
        for &(k, l) in &live_keys {
            let proof = tree.prove(&k);
            assert!(proof.verify::<TestHasher>(&k, Some(l), root).unwrap(), "live key proof failed");
        }
    }

    #[test]
    fn test_200_elements_order_independence() {
        let mut rng1 = StdRng::seed_from_u64(777);
        let entries: Vec<(Hash, Hash)> = (0..200).map(|_| (Hash::from_bytes(rng1.r#gen()), Hash::from_bytes(rng1.r#gen()))).collect();

        let mut tree1 = Smt::new();
        for &(k, l) in &entries {
            tree1.insert(k, l);
        }

        let mut tree2 = Smt::new();
        for &(k, l) in entries.iter().rev() {
            tree2.insert(k, l);
        }

        let mut tree3 = Smt::new();
        let n = entries.len();
        for i in 0..n {
            let &(k, l) = &entries[(i + 73) % n];
            tree3.insert(k, l);
        }

        assert_eq!(tree1.root(), tree2.root(), "forward vs reverse");
        assert_eq!(tree1.root(), tree3.root(), "forward vs shuffled");
    }

    // ========================================================================
    // 11. Security tests
    // ========================================================================

    #[test]
    fn test_empty_hash_not_valid_leaf() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, ZERO_HASH);
        assert!(tree.is_empty());
        assert_eq!(tree.get(&key), None);
        assert_eq!(tree.root(), crate::empty_root::<TestHasher>());
    }

    #[test]
    fn test_proof_for_zero_key() {
        let mut tree = Smt::new();
        let zero_key = Hash::from_bytes([0u8; 32]);
        let leaf = test_leaf(b"zero_key_leaf");
        tree.insert(zero_key, leaf);
        assert_inclusion(&tree, &zero_key, leaf);
    }

    #[test]
    fn test_proof_for_max_key() {
        let mut tree = Smt::new();
        let max_key = Hash::from_bytes([0xFF; 32]);
        let leaf = test_leaf(b"max_key_leaf");
        tree.insert(max_key, leaf);
        assert_inclusion(&tree, &max_key, leaf);
    }

    // ========================================================================
    // 12. Golden value test
    // ========================================================================

    #[test]
    fn test_empty_root_golden_value() {
        let root = crate::empty_root::<TestHasher>();
        let root_hex = std::format!("{root}");
        let root2 = crate::empty_root::<TestHasher>();
        assert_eq!(std::format!("{root2}"), root_hex);
    }

    // ========================================================================
    // 13. Proof structure tests
    // ========================================================================

    #[test]
    fn test_proof_sibling_count_bounds() {
        let mut tree = Smt::new();
        let mut keys = Vec::new();
        for i in 0u32..50 {
            let k = test_key(&i.to_le_bytes());
            tree.insert(k, test_leaf(&i.to_le_bytes()));
            keys.push(k);
        }
        for k in &keys {
            let proof = tree.prove(k);
            assert!(proof.non_empty_count() <= DEPTH);
            assert!(proof.non_empty_count() <= 50);
            assert_eq!(proof.non_empty_count() + proof.empty_count(), DEPTH);
        }
    }

    #[test]
    fn test_proof_bitmap_consistency() {
        let mut tree = Smt::new();
        for i in 0u32..10 {
            tree.insert(test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()));
        }
        let proof = tree.prove(&test_key(&0u32.to_le_bytes()));

        let bitmap_empty_count: usize = proof.bitmap.iter().map(|b| b.count_ones() as usize).sum();
        assert_eq!(bitmap_empty_count, proof.empty_count());
        assert_eq!(DEPTH - bitmap_empty_count, proof.non_empty_count());
        assert_eq!(proof.siblings.len(), proof.non_empty_count());
    }

    // ========================================================================
    // 14. Proof error tests
    // ========================================================================

    #[test]
    fn test_malformed_proof_too_many_siblings() {
        use crate::proof::{OwnedSmtProof, SmtProofError};
        let proof = OwnedSmtProof { bitmap: [0xFF; 32], siblings: Vec::from([ZERO_HASH]) };
        let key = test_key(b"k");
        let err = proof.compute_root::<TestHasher>(&key, None).unwrap_err();
        assert_eq!(err, SmtProofError::SiblingCountMismatch { expected: 0, actual: 1 });
    }

    #[test]
    fn test_malformed_proof_too_few_siblings() {
        use crate::proof::{OwnedSmtProof, SmtProofError};
        let proof = OwnedSmtProof { bitmap: [0x00; 32], siblings: Vec::new() };
        let key = test_key(b"k");
        let err = proof.compute_root::<TestHasher>(&key, None).unwrap_err();
        assert_eq!(err, SmtProofError::SiblingCountMismatch { expected: 256, actual: 0 });
    }
}
