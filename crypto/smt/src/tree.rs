//! Full Sparse Merkle Tree with incremental insert/remove, cached root, and proof generation.
//!
//! This module requires the `std` feature and is **not** intended for ZK guest programs.
//! Use [`crate::proof::SmtProof`] for ZK-compatible proof verification.
//!
//! # Storage
//!
//! The tree is generic over [`SmtStore`], which abstracts both leaf and branch
//! node storage. The default store is [`BTreeSmtStore`] (in-memory `BTreeMap`).
//!
//! # Incremental updates
//!
//! `insert` and `remove` walk from leaf to root (O(256) hashes), updating
//! stored branch nodes along the path. The root is cached and returned in O(1).
//!
//! # Semantics
//!
//! Inserting [`ZERO_HASH`] as a leaf value is equivalent to removing the key,
//! since `ZERO_HASH` is the canonical empty leaf marker.

use std::vec::Vec;

use core::marker::PhantomData;
use kaspa_hashes::{Hash, ZERO_HASH};

use crate::proof::OwnedSmtProof;
use crate::store::{BTreeSmtStore, BranchKey, SmtStore};
use crate::{DEPTH, SmtHasher, bit_at, hash_node};

/// A 256-bit Sparse Merkle Tree with incremental updates and cached root.
///
/// Generic over:
/// - `H`: internal node hasher (e.g., `SeqCommitActiveNode`)
/// - `S`: storage backend (default: [`BTreeSmtStore`])
pub struct SparseMerkleTree<H: SmtHasher, S: SmtStore = BTreeSmtStore> {
    store: S,
    root: Hash,
    _phantom: PhantomData<H>,
}

impl<H: SmtHasher> SparseMerkleTree<H, BTreeSmtStore> {
    /// Create a new empty sparse Merkle tree with the default in-memory store.
    pub fn new() -> Self {
        Self { store: BTreeSmtStore::new(), root: H::EMPTY_HASHES[DEPTH], _phantom: PhantomData }
    }
}

impl<H: SmtHasher> Default for SparseMerkleTree<H, BTreeSmtStore> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: SmtHasher, S: SmtStore> SparseMerkleTree<H, S> {
    /// Create a new empty sparse Merkle tree with a custom store.
    pub fn with_store(store: S) -> Self {
        Self { store, root: H::EMPTY_HASHES[DEPTH], _phantom: PhantomData }
    }

    /// Insert or update a leaf, incrementally updating the root.
    ///
    /// If `leaf_hash` is [`ZERO_HASH`], the key is removed instead.
    /// Returns `Ok(())` on success.
    pub fn insert(&mut self, key: Hash, leaf_hash: Hash) -> Result<(), S::Error> {
        if leaf_hash == ZERO_HASH {
            self.remove(&key)?;
            return Ok(());
        }
        self.store.insert_leaf(key, leaf_hash)?;
        self.root = self.walk_up(&key, leaf_hash)?;
        Ok(())
    }

    /// Remove a leaf by key, incrementally updating the root.
    ///
    /// Returns the previous leaf hash, or `None` if the key was absent.
    pub fn remove(&mut self, key: &Hash) -> Result<Option<Hash>, S::Error> {
        let old = self.store.remove_leaf(key)?;
        if old.is_some() {
            self.root = self.walk_up(key, ZERO_HASH)?;
        }
        Ok(old)
    }

    /// Return the current root hash (cached, O(1)).
    pub fn root(&self) -> Hash {
        self.root
    }

    /// Look up the leaf hash for a key, or `None` if absent.
    pub fn get(&self, key: &Hash) -> Result<Option<Hash>, S::Error> {
        self.store.get_leaf(key)
    }

    /// Number of non-empty leaves (only available on stores that expose this).
    pub fn is_empty(&self) -> Result<bool, S::Error> {
        self.store.is_empty_leaves()
    }

    /// Generate an inclusion or non-inclusion proof for the given key.
    ///
    /// Walks from leaf to root reading stored branch nodes — O(256) reads.
    pub fn prove(&self, key: &Hash) -> Result<OwnedSmtProof, S::Error> {
        let empty_hashes = &H::EMPTY_HASHES;
        let mut bitmap = [0u8; 32];
        let mut siblings = Vec::new();

        for depth in 0..DEPTH {
            let height = DEPTH - 1 - depth; // height from leaf
            let goes_right = bit_at(key, depth);

            // Branch key: height = children's level (same convention as walk_up).
            let branch_key = BranchKey::new(height as u8, key);

            let sibling = if let Some((left, right)) = self.store.get_branch(&branch_key)? {
                if goes_right { left } else { right }
            } else {
                // No branch stored → both children are empty subtree hashes.
                empty_hashes[height]
            };

            if sibling == empty_hashes[height] {
                bitmap[depth / 8] |= 1 << (depth % 8);
            } else {
                siblings.push(sibling);
            }
        }

        Ok(OwnedSmtProof { bitmap, siblings })
    }

    /// Check whether a key is present in the tree.
    pub fn contains_key(&self, key: &Hash) -> Result<bool, S::Error> {
        Ok(self.store.get_leaf(key)?.is_some())
    }

    /// Walk from leaf to root, updating branch nodes and returning the new root.
    fn walk_up(&mut self, key: &Hash, leaf_hash: Hash) -> Result<Hash, S::Error> {
        let empty_hashes = &H::EMPTY_HASHES;
        let mut current = leaf_hash;

        for height in 0..DEPTH {
            let depth = DEPTH - 1 - height;
            let goes_right = bit_at(key, depth);

            // Branch key: height = children's level (0 = leaf parent, 255 = root).
            let branch_key = BranchKey::new(height as u8, key);

            // Get the sibling hash from the currently stored branch,
            // or fall back to the empty hash for this height.
            let sibling = if let Some((left, right)) = self.store.get_branch(&branch_key)? {
                if goes_right { left } else { right }
            } else {
                empty_hashes[height]
            };

            let (left, right) = if goes_right { (sibling, current) } else { (current, sibling) };
            let parent_hash = hash_node::<H>(left, right);

            if parent_hash == empty_hashes[height + 1] {
                self.store.remove_branch(&branch_key)?;
            } else {
                self.store.insert_branch(branch_key, left, right)?;
            }

            current = parent_hash;
        }

        Ok(current)
    }
}

// Convenience methods for BTreeSmtStore (infallible).
#[allow(clippy::len_without_is_empty)]
impl<H: SmtHasher> SparseMerkleTree<H, BTreeSmtStore> {
    /// Number of non-empty leaves.
    pub fn len(&self) -> usize {
        self.store.leaf_count()
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

    fn test_key(seed: &[u8]) -> Hash {
        let mut h = TestHasher::default();
        h.update(b"test_key:");
        h.update(seed);
        h.finalize()
    }

    fn test_leaf(seed: &[u8]) -> Hash {
        let mut h = TestHasher::default();
        h.update(b"test_leaf:");
        h.update(seed);
        h.finalize()
    }

    fn key_from_bytes(bytes: [u8; 32]) -> Hash {
        Hash::from_bytes(bytes)
    }

    fn assert_inclusion(tree: &Smt, key: &Hash, leaf: Hash) {
        let root = tree.root();
        let proof = tree.prove(key).unwrap();
        assert!(proof.verify::<TestHasher>(key, Some(leaf), root).unwrap(), "inclusion proof failed for key {key}");
        assert_eq!(proof.compute_root::<TestHasher>(key, Some(leaf)).unwrap(), root, "compute_root mismatch for key {key}");
    }

    fn assert_non_inclusion(tree: &Smt, key: &Hash) {
        let root = tree.root();
        let proof = tree.prove(key).unwrap();
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
        assert_ne!(tree.root(), ZERO_HASH, "empty root should not be ZERO_HASH");
    }

    #[test]
    fn test_empty_tree_properties() {
        let tree = Smt::new();
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty().unwrap());
    }

    #[test]
    fn test_empty_tree_get_returns_none() {
        let tree = Smt::new();
        let key = test_key(b"any");
        assert_eq!(tree.get(&key).unwrap(), None);
        assert!(!tree.contains_key(&key).unwrap());
    }

    #[test]
    fn test_empty_tree_remove_returns_none() {
        let mut tree = Smt::new();
        let key = test_key(b"any");
        assert_eq!(tree.remove(&key).unwrap(), None);
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
        let proof = tree.prove(&key).unwrap();
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
        tree.insert(test_key(b"k"), test_leaf(b"v")).unwrap();
        assert_ne!(tree.root(), empty_root);
    }

    #[test]
    fn test_single_get() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf).unwrap();
        assert_eq!(tree.get(&key).unwrap(), Some(leaf));
        assert!(tree.contains_key(&key).unwrap());
        assert_eq!(tree.len(), 1);
        assert!(!tree.is_empty().unwrap());
    }

    #[test]
    fn test_single_inclusion_proof() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf).unwrap();
        assert_inclusion(&tree, &key, leaf);
    }

    #[test]
    fn test_single_non_inclusion_for_other_key() {
        let mut tree = Smt::new();
        tree.insert(test_key(b"present"), test_leaf(b"v")).unwrap();
        let absent = test_key(b"absent");
        assert_non_inclusion(&tree, &absent);
    }

    #[test]
    fn test_single_delete_restores_empty() {
        let mut tree = Smt::new();
        let empty_root = tree.root();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v")).unwrap();
        assert_ne!(tree.root(), empty_root);
        tree.remove(&key).unwrap();
        assert_eq!(tree.root(), empty_root);
        assert!(tree.is_empty().unwrap());
    }

    #[test]
    fn test_single_insert_delete_reinsert() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf).unwrap();
        let root_after_insert = tree.root();
        tree.remove(&key).unwrap();
        tree.insert(key, leaf).unwrap();
        assert_eq!(tree.root(), root_after_insert);
    }

    #[test]
    fn test_single_update_changes_root() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v1")).unwrap();
        let root1 = tree.root();
        tree.insert(key, test_leaf(b"v2")).unwrap();
        let root2 = tree.root();
        assert_ne!(root1, root2);
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn test_insert_zero_hash_is_remove() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v")).unwrap();
        assert_eq!(tree.len(), 1);
        tree.insert(key, ZERO_HASH).unwrap();
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty().unwrap());
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
        tree.insert(k1, l1).unwrap();
        tree.insert(k2, l2).unwrap();
        assert_eq!(tree.get(&k1).unwrap(), Some(l1));
        assert_eq!(tree.get(&k2).unwrap(), Some(l2));
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn test_two_elements_both_proofs_verify() {
        let mut tree = Smt::new();
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");
        tree.insert(k1, l1).unwrap();
        tree.insert(k2, l2).unwrap();
        assert_inclusion(&tree, &k1, l1);
        assert_inclusion(&tree, &k2, l2);
    }

    #[test]
    fn test_three_elements_all_proofs_verify() {
        let mut tree = Smt::new();
        let keys: Vec<Hash> = (0..3).map(|i| test_key(&[i])).collect();
        let leaves: Vec<Hash> = (0..3).map(|i| test_leaf(&[i])).collect();
        for (&k, &l) in keys.iter().zip(&leaves) {
            tree.insert(k, l).unwrap();
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
            tree.insert(k, l).unwrap();
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
            tree.insert(test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes())).unwrap();
            let new_root = tree.root();
            assert_ne!(new_root, prev_root, "root didn't change after insert {i}");
            prev_root = new_root;
        }
    }

    #[test]
    fn test_different_values_different_roots() {
        let key = test_key(b"k");
        let mut tree1 = Smt::new();
        tree1.insert(key, test_leaf(b"v1")).unwrap();
        let mut tree2 = Smt::new();
        tree2.insert(key, test_leaf(b"v2")).unwrap();
        assert_ne!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_different_keys_different_roots() {
        let leaf = test_leaf(b"same_leaf");
        let mut tree1 = Smt::new();
        tree1.insert(test_key(b"k1"), leaf).unwrap();
        let mut tree2 = Smt::new();
        tree2.insert(test_key(b"k2"), leaf).unwrap();
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
        tree.insert(key, leaf).unwrap();
        assert_eq!(tree.remove(&key).unwrap(), Some(leaf));
    }

    #[test]
    fn test_delete_nonexistent_noop() {
        let mut tree = Smt::new();
        tree.insert(test_key(b"present"), test_leaf(b"v")).unwrap();
        let root_before = tree.root();
        assert_eq!(tree.remove(&test_key(b"absent")).unwrap(), None);
        assert_eq!(tree.root(), root_before);
    }

    #[test]
    fn test_delete_restores_previous_root() {
        let mut tree = Smt::new();
        let ka = test_key(b"a");
        let kb = test_key(b"b");
        tree.insert(ka, test_leaf(b"la")).unwrap();
        let root_a_only = tree.root();
        tree.insert(kb, test_leaf(b"lb")).unwrap();
        assert_ne!(tree.root(), root_a_only);
        tree.remove(&kb).unwrap();
        assert_eq!(tree.root(), root_a_only);
    }

    #[test]
    fn test_delete_all_returns_to_empty() {
        let mut tree = Smt::new();
        let empty_root = tree.root();
        let keys: Vec<Hash> = (0u32..10).map(|i| test_key(&i.to_le_bytes())).collect();
        for (i, &k) in keys.iter().enumerate() {
            tree.insert(k, test_leaf(&(i as u32).to_le_bytes())).unwrap();
        }
        assert_ne!(tree.root(), empty_root);
        for k in &keys {
            tree.remove(k).unwrap();
        }
        assert_eq!(tree.root(), empty_root);
        assert!(tree.is_empty().unwrap());
    }

    #[test]
    fn test_delete_middle_element() {
        let mut tree = Smt::new();
        let ka = test_key(b"a");
        let kb = test_key(b"b");
        let kc = test_key(b"c");
        let la = test_leaf(b"la");
        let lc = test_leaf(b"lc");
        tree.insert(ka, la).unwrap();
        tree.insert(kb, test_leaf(b"lb")).unwrap();
        tree.insert(kc, lc).unwrap();
        tree.remove(&kb).unwrap();
        assert_eq!(tree.len(), 2);
        assert_inclusion(&tree, &ka, la);
        assert_inclusion(&tree, &kc, lc);
        assert_non_inclusion(&tree, &kb);
    }

    #[test]
    fn test_non_inclusion_after_delete() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v")).unwrap();
        tree.remove(&key).unwrap();
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
            tree1.insert(k, l).unwrap();
        }

        let mut tree2 = Smt::new();
        for &(k, l) in entries.iter().rev() {
            tree2.insert(k, l).unwrap();
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
            tree1.insert(k, l).unwrap();
            tree2.insert(k, l).unwrap();
        }
        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_root_idempotent() {
        let mut tree = Smt::new();
        for i in 0u32..5 {
            tree.insert(test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes())).unwrap();
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
        tree1.insert(key, leaf).unwrap();

        let mut tree2 = Smt::new();
        tree2.insert(key, leaf).unwrap();
        tree2.remove(&key).unwrap();
        tree2.insert(key, leaf).unwrap();

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
            tree.insert(k, l).unwrap();
        }
        for &(k, l) in &entries {
            assert_inclusion(&tree, &k, l);
        }
    }

    #[test]
    fn test_proof_after_update() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        tree.insert(key, test_leaf(b"v1")).unwrap();
        let new_leaf = test_leaf(b"v2");
        tree.insert(key, new_leaf).unwrap();
        assert_inclusion(&tree, &key, new_leaf);
    }

    #[test]
    fn test_proof_compute_root_matches_tree_root() {
        let mut tree = Smt::new();
        let entries: Vec<(Hash, Hash)> = (0u32..5).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();
        for &(k, l) in &entries {
            tree.insert(k, l).unwrap();
        }
        let root = tree.root();
        for &(k, l) in &entries {
            let proof = tree.prove(&k).unwrap();
            assert_eq!(proof.compute_root::<TestHasher>(&k, Some(l)).unwrap(), root);
        }
    }

    #[test]
    fn test_inclusion_and_non_inclusion_coexist() {
        let mut tree = Smt::new();
        let present: Vec<(Hash, Hash)> = (0u32..5).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();
        let absent: Vec<Hash> = (100u32..105).map(|i| test_key(&i.to_le_bytes())).collect();
        for &(k, l) in &present {
            tree.insert(k, l).unwrap();
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
        tree.insert(k, test_leaf(b"v")).unwrap();
        let proof = tree.prove(&k).unwrap();
        assert_eq!(proof.non_empty_count(), 0);
        assert_eq!(proof.empty_count(), DEPTH);

        tree.insert(test_key(b"other"), test_leaf(b"w")).unwrap();
        let proof = tree.prove(&k).unwrap();
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
        tree.insert(test_key(b"present"), test_leaf(b"v")).unwrap();
        assert_non_inclusion(&tree, &test_key(b"absent"));
    }

    #[test]
    fn test_non_inclusion_multi_element_tree() {
        let mut tree = Smt::new();
        for i in 0u32..10 {
            tree.insert(test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes())).unwrap();
        }
        for i in 100u32..110 {
            assert_non_inclusion(&tree, &test_key(&i.to_le_bytes()));
        }
    }

    #[test]
    fn test_non_inclusion_after_deletion() {
        let mut tree = Smt::new();
        let key = test_key(b"will_delete");
        tree.insert(key, test_leaf(b"v")).unwrap();
        tree.insert(test_key(b"stays"), test_leaf(b"w")).unwrap();
        tree.remove(&key).unwrap();
        assert_non_inclusion(&tree, &key);
    }

    #[test]
    fn test_non_inclusion_proof_is_different_from_inclusion() {
        let mut tree = Smt::new();
        let present_key = test_key(b"present");
        let present_leaf = test_leaf(b"v");
        let absent_key = test_key(b"absent");
        tree.insert(present_key, present_leaf).unwrap();

        let root = tree.root();
        let inclusion_proof = tree.prove(&present_key).unwrap();
        let non_inclusion_proof = tree.prove(&absent_key).unwrap();

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
        tree.insert(key, test_leaf(b"correct")).unwrap();
        let root = tree.root();
        let proof = tree.prove(&key).unwrap();
        let wrong_leaf = test_leaf(b"wrong");
        assert!(!proof.verify::<TestHasher>(&key, Some(wrong_leaf), root).unwrap());
    }

    #[test]
    fn test_reject_wrong_root() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf).unwrap();
        let proof = tree.prove(&key).unwrap();
        let wrong_root = test_leaf(b"fake_root");
        assert!(!proof.verify::<TestHasher>(&key, Some(leaf), wrong_root).unwrap());
    }

    #[test]
    fn test_reject_wrong_key() {
        let mut tree = Smt::new();
        let key = test_key(b"correct_key");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf).unwrap();
        let root = tree.root();
        let proof = tree.prove(&key).unwrap();
        let wrong_key = test_key(b"wrong_key");
        assert!(!proof.verify::<TestHasher>(&wrong_key, Some(leaf), root).unwrap());
    }

    #[test]
    fn test_reject_tampered_sibling() {
        let mut tree = Smt::new();
        let k1 = test_key(b"k1");
        let k2 = test_key(b"k2");
        tree.insert(k1, test_leaf(b"v1")).unwrap();
        tree.insert(k2, test_leaf(b"v2")).unwrap();
        let root = tree.root();
        let mut proof = tree.prove(&k1).unwrap();

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
        tree.insert(key, leaf).unwrap();
        let root = tree.root();
        let proof = tree.prove(&key).unwrap();
        assert!(!proof.verify::<TestHasher>(&key, None, root).unwrap());
    }

    #[test]
    fn test_reject_non_inclusion_as_inclusion() {
        let mut tree = Smt::new();
        tree.insert(test_key(b"other"), test_leaf(b"w")).unwrap();
        let absent = test_key(b"absent");
        let root = tree.root();
        let proof = tree.prove(&absent).unwrap();
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
        tree.insert(k1, l1).unwrap();
        tree.insert(k2, l2).unwrap();

        assert_inclusion(&tree, &k1, l1);
        assert_inclusion(&tree, &k2, l2);

        let proof1 = tree.prove(&k1).unwrap();
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
        tree.insert(k1, l1).unwrap();
        tree.insert(k2, l2).unwrap();

        assert_inclusion(&tree, &k1, l1);
        assert_inclusion(&tree, &k2, l2);

        let proof1 = tree.prove(&k1).unwrap();
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
        tree.insert(k1, test_leaf(b"v1")).unwrap();
        tree.insert(k2, test_leaf(b"v2")).unwrap();

        assert_inclusion(&tree, &k1, test_leaf(b"v1"));
        assert_inclusion(&tree, &k2, test_leaf(b"v2"));
    }

    #[test]
    fn test_zero_key_and_max_key() {
        let zero_key = key_from_bytes([0x00; 32]);
        let max_key = key_from_bytes([0xFF; 32]);
        let mut tree = Smt::new();
        tree.insert(zero_key, test_leaf(b"zero")).unwrap();
        tree.insert(max_key, test_leaf(b"max")).unwrap();

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
            tree.insert(key, leaf).unwrap();
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
        tree.insert(k1, test_leaf(b"aa")).unwrap();
        tree.insert(k2, test_leaf(b"55")).unwrap();
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
            tree.insert(key, leaf).unwrap();
            entries.push((key, leaf));
        }

        let root = tree.root();
        for &(k, l) in &entries {
            let proof = tree.prove(&k).unwrap();
            assert!(proof.verify::<TestHasher>(&k, Some(l), root).unwrap(), "inclusion proof failed");
        }
    }

    #[test]
    fn test_100_random_non_inclusion() {
        let mut rng = StdRng::seed_from_u64(123);
        let mut tree = Smt::new();

        for _ in 0..100 {
            tree.insert(Hash::from_bytes(rng.r#gen()), Hash::from_bytes(rng.r#gen())).unwrap();
        }

        let root = tree.root();

        let mut rng2 = StdRng::seed_from_u64(456);
        for _ in 0..100 {
            let absent_key = Hash::from_bytes(rng2.r#gen());
            if tree.contains_key(&absent_key).unwrap() {
                continue;
            }
            let proof = tree.prove(&absent_key).unwrap();
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
                tree.remove(&key).unwrap();
            } else {
                let key = Hash::from_bytes(rng.r#gen());
                let leaf = Hash::from_bytes(rng.r#gen());
                tree.insert(key, leaf).unwrap();
                live_keys.push((key, leaf));
            }
        }

        let root = tree.root();
        for &(k, l) in &live_keys {
            let proof = tree.prove(&k).unwrap();
            assert!(proof.verify::<TestHasher>(&k, Some(l), root).unwrap(), "live key proof failed");
        }
    }

    #[test]
    fn test_200_elements_order_independence() {
        let mut rng1 = StdRng::seed_from_u64(777);
        let entries: Vec<(Hash, Hash)> = (0..200).map(|_| (Hash::from_bytes(rng1.r#gen()), Hash::from_bytes(rng1.r#gen()))).collect();

        let mut tree1 = Smt::new();
        for &(k, l) in &entries {
            tree1.insert(k, l).unwrap();
        }

        let mut tree2 = Smt::new();
        for &(k, l) in entries.iter().rev() {
            tree2.insert(k, l).unwrap();
        }

        let mut tree3 = Smt::new();
        let n = entries.len();
        for i in 0..n {
            let &(k, l) = &entries[(i + 73) % n];
            tree3.insert(k, l).unwrap();
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
        tree.insert(key, ZERO_HASH).unwrap();
        assert!(tree.is_empty().unwrap());
        assert_eq!(tree.get(&key).unwrap(), None);
        assert_eq!(tree.root(), crate::empty_root::<TestHasher>());
    }

    #[test]
    fn test_proof_for_zero_key() {
        let mut tree = Smt::new();
        let zero_key = Hash::from_bytes([0u8; 32]);
        let leaf = test_leaf(b"zero_key_leaf");
        tree.insert(zero_key, leaf).unwrap();
        assert_inclusion(&tree, &zero_key, leaf);
    }

    #[test]
    fn test_proof_for_max_key() {
        let mut tree = Smt::new();
        let max_key = Hash::from_bytes([0xFF; 32]);
        let leaf = test_leaf(b"max_key_leaf");
        tree.insert(max_key, leaf).unwrap();
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
            tree.insert(k, test_leaf(&i.to_le_bytes())).unwrap();
            keys.push(k);
        }
        for k in &keys {
            let proof = tree.prove(k).unwrap();
            assert!(proof.non_empty_count() <= DEPTH);
            assert!(proof.non_empty_count() <= 50);
            assert_eq!(proof.non_empty_count() + proof.empty_count(), DEPTH);
        }
    }

    #[test]
    fn test_proof_bitmap_consistency() {
        let mut tree = Smt::new();
        for i in 0u32..10 {
            tree.insert(test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes())).unwrap();
        }
        let proof = tree.prove(&test_key(&0u32.to_le_bytes())).unwrap();

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
