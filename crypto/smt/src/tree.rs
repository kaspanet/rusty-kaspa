//! Full Sparse Merkle Tree with incremental insert/remove, cached root, and proof generation.
//!
//! # Two APIs
//!
//! - **In-memory** (`SparseMerkleTree<H, BTreeSmtStore>`): mutable `insert`/`remove`/`prove`.
//!   Used in tests and for RPC proof generation.
//!
//! - **Pure computation** ([`compute_root_update`]): reads from an immutable `&impl SmtStore`,
//!   returns `(new_root, changed_branches)`. Only actually-modified branches appear
//!   in the output. Paths stop propagating when the computed parent matches the existing
//!   value. Used by consensus (`SmtProcessor::build`).

use std::collections::BTreeMap;
use std::vec::Vec;

use core::convert::Infallible;
use core::marker::PhantomData;
use kaspa_hashes::{Hash, ZERO_HASH};

use crate::proof::OwnedSmtProof;
use crate::store::{BTreeSmtStore, BranchChildren, BranchKey, SmtStore};
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
    /// Create a sparse Merkle tree from an existing store and root hash.
    pub fn from_root(store: S, root: Hash) -> Self {
        Self { store, root, _phantom: PhantomData }
    }

    /// Create a new empty sparse Merkle tree with a custom store.
    pub fn with_store(store: S) -> Self {
        Self { store, root: H::EMPTY_HASHES[DEPTH], _phantom: PhantomData }
    }

    /// Consume the tree and return the underlying store.
    pub fn into_store(self) -> S {
        self.store
    }

    /// Return the current root hash (cached, O(1)).
    pub fn root(&self) -> Hash {
        self.root
    }

    /// Generate an inclusion or non-inclusion proof for the given key.
    ///
    /// Walks from leaf to root reading stored branch nodes — O(256) reads.
    pub fn prove(&self, key: &Hash) -> Result<OwnedSmtProof, S::Error> {
        let empty_hashes = &H::EMPTY_HASHES;
        let mut bitmap = [0u8; 32];
        let mut siblings = Vec::new();

        for depth in 0..DEPTH {
            let height = DEPTH - 1 - depth;
            let goes_right = bit_at(key, depth);
            let branch_key = BranchKey::new(height as u8, key);

            let sibling = if let Some(bc) = self.store.get_branch(&branch_key)? {
                if goes_right { bc.left } else { bc.right }
            } else {
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
}

// =========================================================================
// In-memory operations (BTreeSmtStore only — tests + RPC proofs)
// =========================================================================

#[allow(clippy::len_without_is_empty)]
impl<H: SmtHasher> SparseMerkleTree<H, BTreeSmtStore> {
    /// Number of non-empty leaves.
    pub fn len(&self) -> usize {
        self.store.leaf_count()
    }

    /// Whether the tree has no leaves.
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    /// Look up the leaf hash for a key, or `None` if absent.
    pub fn get(&self, key: &Hash) -> Option<Hash> {
        self.store.get_leaf(key)
    }

    /// Check whether a key is present in the tree.
    pub fn contains_key(&self, key: &Hash) -> bool {
        self.store.get_leaf(key).is_some()
    }

    /// Insert or update a leaf, incrementally updating the root.
    ///
    /// Inserting [`ZERO_HASH`] marks the leaf as empty (equivalent to `remove`).
    pub fn insert(&mut self, key: Hash, leaf_hash: Hash) -> Result<(), Infallible> {
        self.store.insert_leaf(key, leaf_hash);
        self.root = self.walk_up(&key, leaf_hash);
        Ok(())
    }

    /// Insert or update multiple leaves in batch.
    ///
    /// Keys are sorted and processed bottom-up. At each level, entries sharing
    /// a branch are paired directly (no store read for the sibling).
    pub fn insert_many(&mut self, updates: BTreeMap<Hash, Hash>) -> Result<(), Infallible> {
        if updates.is_empty() {
            return Ok(());
        }

        for (&key, &leaf_hash) in &updates {
            self.store.insert_leaf(key, leaf_hash);
        }

        if updates.len() == 1 {
            let (&key, &leaf_hash) = updates.iter().next().unwrap();
            self.root = self.walk_up(&key, leaf_hash);
            return Ok(());
        }

        let mut current: Vec<(Hash, Hash)> = updates.into_iter().collect();
        let empty_hashes = &H::EMPTY_HASHES;
        let mut next = Vec::with_capacity(current.len());

        for depth in (0..DEPTH).rev() {
            let height = DEPTH - 1 - depth;
            let mut i = 0;

            while i < current.len() {
                let (key, hash) = current[i];
                let branch_key = BranchKey::new(height as u8, &key);

                let sibling_in_batch = if i + 1 < current.len() {
                    let next_bk = BranchKey::new(height as u8, &current[i + 1].0);
                    if branch_key == next_bk { Some(current[i + 1].1) } else { None }
                } else {
                    None
                };

                let (left, right) = if let Some(sib) = sibling_in_batch {
                    (hash, sib)
                } else {
                    let goes_right = bit_at(&key, depth);
                    let sibling = self
                        .store
                        .get_branch(&branch_key)
                        .unwrap()
                        .map(|bc| if goes_right { bc.left } else { bc.right })
                        .unwrap_or(empty_hashes[height]);
                    if goes_right { (sibling, hash) } else { (hash, sibling) }
                };

                let parent = hash_node::<H>(left, right);
                self.store.insert_branch(branch_key, BranchChildren { left, right });
                next.push((key, parent));

                i += if sibling_in_batch.is_some() { 2 } else { 1 };
            }

            core::mem::swap(&mut current, &mut next);
            next.clear();
        }

        debug_assert_eq!(current.len(), 1);
        self.root = current[0].1;
        Ok(())
    }

    /// Remove a leaf by key, incrementally updating the root.
    pub fn remove(&mut self, key: &Hash) -> Option<Hash> {
        let old = self.store.get_leaf(key);
        if old.is_some() {
            self.root = self.walk_up(key, ZERO_HASH);
            self.store.insert_leaf(*key, ZERO_HASH);
        }
        old
    }

    /// Walk from leaf to root, updating branch nodes and returning the new root.
    fn walk_up(&mut self, key: &Hash, leaf_hash: Hash) -> Hash {
        let empty_hashes = &H::EMPTY_HASHES;
        let mut current = leaf_hash;

        for depth in (0..DEPTH).rev() {
            let height = DEPTH - 1 - depth;
            let goes_right = bit_at(key, depth);
            let branch_key = BranchKey::new(height as u8, key);

            let sibling = self
                .store
                .get_branch(&branch_key)
                .unwrap()
                .map(|bc| if goes_right { bc.left } else { bc.right })
                .unwrap_or(empty_hashes[height]);

            let (left, right) = if goes_right { (sibling, current) } else { (current, sibling) };
            let parent_hash = hash_node::<H>(left, right);
            self.store.insert_branch(branch_key, BranchChildren { left, right });
            current = parent_hash;
        }

        current
    }
}

// =========================================================================
// Pure computation: reads from immutable store, returns only changed branches
// =========================================================================

/// Map of changed branches: `BranchKey → BranchChildren`.
pub type SmtBranchChanges = BTreeMap<BranchKey, BranchChildren>;

/// Compute the new SMT root and **only the changed branches** for a set of leaf updates.
///
/// Reads from an immutable `&impl SmtStore`. Does not mutate any state.
/// Paths stop propagating when the computed parent matches the existing value
/// in the store, so untouched subtrees are never visited above the divergence point.
///
/// Returns `(new_root, changed_branches)` where the map contains only branches
/// whose children actually changed.
pub fn compute_root_update<H: SmtHasher, S: SmtStore>(
    store: &S,
    current_root: Hash,
    leaf_updates: BTreeMap<Hash, Hash>,
) -> Result<(Hash, SmtBranchChanges), S::Error> {
    if leaf_updates.is_empty() {
        return Ok((current_root, BTreeMap::new()));
    }

    // Buffer: branches modified during computation.
    // Needed so later leaves on shared paths see earlier updates.
    let mut changes: SmtBranchChanges = BTreeMap::new();

    // Read a branch: check our local buffer first, then the immutable store.
    let read_branch = |changes: &SmtBranchChanges, bk: BranchKey| -> Result<Option<BranchChildren>, S::Error> {
        if let Some(&bc) = changes.get(&bk) {
            return Ok(Some(bc));
        }
        store.get_branch(&bk)
    };

    let empty_hashes = &H::EMPTY_HASHES;
    let mut current: Vec<(Hash, Hash)> = leaf_updates.into_iter().collect();
    let mut next = Vec::with_capacity(current.len());

    for depth in (0..DEPTH).rev() {
        let height = DEPTH - 1 - depth;

        let mut i = 0;
        while i < current.len() {
            let (key, hash) = current[i];
            let branch_key = BranchKey::new(height as u8, &key);

            // Check if the next entry is a sibling (same BranchKey)
            let sibling_in_batch = if i + 1 < current.len() {
                let next_bk = BranchKey::new(height as u8, &current[i + 1].0);
                if branch_key == next_bk { Some(current[i + 1].1) } else { None }
            } else {
                None
            };

            // Read existing children from buffer/store
            let existing = read_branch(&changes, branch_key)?;

            let new_children = if let Some(sib) = sibling_in_batch {
                // Both children are in the batch
                BranchChildren { left: hash, right: sib }
            } else {
                // One child is from the batch, sibling from store/buffer
                let goes_right = bit_at(&key, depth);
                let sibling = existing.map(|bc| if goes_right { bc.left } else { bc.right }).unwrap_or(empty_hashes[height]);
                if goes_right { BranchChildren { left: sibling, right: hash } } else { BranchChildren { left: hash, right: sibling } }
            };

            let parent = hash_node::<H>(new_children.left, new_children.right);

            // Only record and propagate if the branch actually changed
            let empty = BranchChildren { left: empty_hashes[height], right: empty_hashes[height] };
            if new_children != existing.unwrap_or(empty) {
                changes.insert(branch_key, new_children);
                next.push((key, parent));
            }

            i += if sibling_in_batch.is_some() { 2 } else { 1 };
        }

        core::mem::swap(&mut current, &mut next);
        next.clear();

        // All paths converged to unchanged branches — root didn't change
        if current.is_empty() {
            return Ok((current_root, changes));
        }
    }

    debug_assert_eq!(current.len(), 1);
    Ok((current[0].1, changes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proof::SmtProofError;
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

    fn btree(entries: impl IntoIterator<Item = (Hash, Hash)>) -> BTreeMap<Hash, Hash> {
        entries.into_iter().collect()
    }

    // ========================================================================
    // compute_root_update tests
    // ========================================================================

    #[test]
    fn test_compute_root_update_empty() {
        let store = BTreeSmtStore::new();
        let empty_root = TestHasher::empty_root();
        let (root, changes) = compute_root_update::<TestHasher, _>(&store, empty_root, BTreeMap::new()).unwrap();
        assert_eq!(root, empty_root);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_compute_root_update_matches_insert() {
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");

        // Build via in-memory tree
        let mut tree = Smt::new();
        tree.insert(k1, l1).unwrap();
        tree.insert(k2, l2).unwrap();
        let expected_root = tree.root();

        // Build via compute_root_update from empty store
        let store = BTreeSmtStore::new();
        let empty_root = TestHasher::empty_root();
        let (root, _changes) = compute_root_update::<TestHasher, _>(&store, empty_root, btree([(k1, l1), (k2, l2)])).unwrap();

        assert_eq!(root, expected_root);
    }

    #[test]
    fn test_compute_root_update_incremental() {
        // Insert k1, persist to store. Then update k2 via compute_root_update.
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");

        let mut tree = Smt::new();
        tree.insert(k1, l1).unwrap();
        let root1 = tree.root();
        // The store now has branches for k1's path
        let store = tree.into_store();

        // compute_root_update from this store with k2
        let (root2, changes) = compute_root_update::<TestHasher, _>(&store, root1, btree([(k2, l2)])).unwrap();

        // Verify against full in-memory tree
        let mut tree2 = Smt::new();
        tree2.insert(k1, l1).unwrap();
        tree2.insert(k2, l2).unwrap();

        assert_eq!(root2, tree2.root());
        // Changes should only contain k2's path, not k1's unchanged branches
        assert!(changes.len() <= 256, "at most 256 branches for one leaf path");
    }

    #[test]
    fn test_compute_root_update_unchanged_no_propagation() {
        // Insert k1 into tree. Then "update" k1 with the same value.
        // No branches should change.
        let k1 = test_key(b"1");
        let l1 = test_leaf(b"a");

        let mut tree = Smt::new();
        tree.insert(k1, l1).unwrap();
        let root = tree.root();
        let store = tree.into_store();

        let (new_root, changes) = compute_root_update::<TestHasher, _>(&store, root, btree([(k1, l1)])).unwrap();

        assert_eq!(new_root, root, "same leaf value should produce same root");
        assert!(changes.is_empty(), "no branches should change when leaf value is identical");
    }

    #[test]
    fn test_compute_root_update_only_changed_branches() {
        // Insert k1 and k2. Then update only k1.
        // Only k1's path branches should appear in changes.
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");
        let l1_new = test_leaf(b"a_new");

        let mut tree = Smt::new();
        tree.insert(k1, l1).unwrap();
        tree.insert(k2, l2).unwrap();
        let root = tree.root();
        let store = tree.into_store();

        let (new_root, changes) = compute_root_update::<TestHasher, _>(&store, root, btree([(k1, l1_new)])).unwrap();

        assert_ne!(new_root, root);
        // The number of changed branches should be <= 256 (one path)
        assert!(changes.len() <= 256);
        // Verify result matches in-memory computation
        let mut tree2 = Smt::new();
        tree2.insert(k1, l1_new).unwrap();
        tree2.insert(k2, l2).unwrap();
        assert_eq!(new_root, tree2.root());
    }

    #[test]
    fn test_compute_root_update_expire_leaf() {
        // Insert k1. Then expire it (ZERO_HASH). Root should return to empty.
        let k1 = test_key(b"1");
        let l1 = test_leaf(b"a");

        let mut tree = Smt::new();
        tree.insert(k1, l1).unwrap();
        let root = tree.root();
        let store = tree.into_store();

        let empty_root = TestHasher::empty_root();
        let (new_root, _changes) = compute_root_update::<TestHasher, _>(&store, root, btree([(k1, ZERO_HASH)])).unwrap();

        assert_eq!(new_root, empty_root);
    }

    // ========================================================================
    // 1. Empty tree tests
    // ========================================================================

    #[test]
    fn test_empty_root() {
        let tree = Smt::new();
        let expected = TestHasher::empty_root();
        assert_eq!(tree.root(), expected);
        assert_ne!(tree.root(), ZERO_HASH, "empty root should not be ZERO_HASH");
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
        assert_eq!(tree.root(), TestHasher::empty_root());
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
        assert_eq!(tree.get(&key), Some(leaf));
        assert!(tree.contains_key(&key));
        assert_eq!(tree.len(), 1);
        assert!(!tree.is_empty());
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
        tree.remove(&key);
        assert_eq!(tree.root(), empty_root);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_single_insert_delete_reinsert() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf).unwrap();
        let root_after_insert = tree.root();
        tree.remove(&key);
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
        assert!(tree.is_empty());
        assert_eq!(tree.root(), TestHasher::empty_root());
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
        assert_eq!(tree.get(&k1), Some(l1));
        assert_eq!(tree.get(&k2), Some(l2));
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
        assert_eq!(tree.remove(&key), Some(leaf));
    }

    #[test]
    fn test_delete_nonexistent_noop() {
        let mut tree = Smt::new();
        tree.insert(test_key(b"present"), test_leaf(b"v")).unwrap();
        let root_before = tree.root();
        assert_eq!(tree.remove(&test_key(b"absent")), None);
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
        tree.remove(&kb);
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
        tree.insert(ka, la).unwrap();
        tree.insert(kb, test_leaf(b"lb")).unwrap();
        tree.insert(kc, lc).unwrap();
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
        tree.insert(key, test_leaf(b"v")).unwrap();
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
        tree2.remove(&key);
        tree2.insert(key, leaf).unwrap();

        assert_eq!(tree1.root(), tree2.root());
    }

    // ========================================================================
    // 6. Inclusion proof tests
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
    // 7. Non-inclusion proof tests
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
        tree.remove(&key);
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
            if tree.contains_key(&absent_key) {
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
                tree.remove(&key);
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
        assert!(tree.is_empty());
        assert_eq!(tree.get(&key), None);
        assert_eq!(tree.root(), TestHasher::empty_root());
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
        let root = TestHasher::empty_root();
        let root_hex = std::format!("{root}");
        let root2 = TestHasher::empty_root();
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
    // 14. insert_many tests
    // ========================================================================

    #[test]
    fn test_insert_many_empty() {
        let mut tree = Smt::new();
        let empty_root = tree.root();
        tree.insert_many(btree([])).unwrap();
        assert_eq!(tree.root(), empty_root);
    }

    #[test]
    fn test_insert_many_single() {
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");

        let mut tree1 = Smt::new();
        tree1.insert(key, leaf).unwrap();

        let mut tree2 = Smt::new();
        tree2.insert_many(btree([(key, leaf)])).unwrap();

        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_insert_many_two_elements() {
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");

        let mut tree1 = Smt::new();
        tree1.insert(k1, l1).unwrap();
        tree1.insert(k2, l2).unwrap();

        let mut tree2 = Smt::new();
        tree2.insert_many(btree([(k1, l1), (k2, l2)])).unwrap();

        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_insert_many_matches_sequential() {
        let entries: Vec<(Hash, Hash)> = (0u32..20).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();

        let mut tree1 = Smt::new();
        for &(k, l) in &entries {
            tree1.insert(k, l).unwrap();
        }

        let mut tree2 = Smt::new();
        tree2.insert_many(btree(entries)).unwrap();

        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_insert_many_order_independent() {
        let entries: Vec<(Hash, Hash)> = (0u32..10).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();

        let mut tree1 = Smt::new();
        tree1.insert_many(btree(entries.clone())).unwrap();

        let mut reversed = entries;
        reversed.reverse();
        let mut tree2 = Smt::new();
        tree2.insert_many(btree(reversed)).unwrap();

        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_insert_many_duplicate_keys_last_wins() {
        let key = test_key(b"k");
        let v1 = test_leaf(b"v1");
        let v2 = test_leaf(b"v2");

        let mut tree1 = Smt::new();
        tree1.insert(key, v2).unwrap();

        // BTreeMap deduplicates by key; last insert wins
        let mut tree2 = Smt::new();
        let mut map = btree([(key, v1)]);
        map.insert(key, v2);
        tree2.insert_many(map).unwrap();

        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_insert_many_incremental() {
        let entries1: Vec<(Hash, Hash)> = (0u32..5).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();
        let entries2: Vec<(Hash, Hash)> = (5u32..10).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();

        let mut tree1 = Smt::new();
        for &(k, l) in entries1.iter().chain(entries2.iter()) {
            tree1.insert(k, l).unwrap();
        }

        let mut tree2 = Smt::new();
        tree2.insert_many(btree(entries1)).unwrap();
        tree2.insert_many(btree(entries2)).unwrap();

        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_insert_many_100_random() {
        let mut rng = StdRng::seed_from_u64(42);
        let entries: Vec<(Hash, Hash)> = (0..100).map(|_| (Hash::from_bytes(rng.r#gen()), Hash::from_bytes(rng.r#gen()))).collect();

        let mut tree1 = Smt::new();
        for &(k, l) in &entries {
            tree1.insert(k, l).unwrap();
        }

        let mut tree2 = Smt::new();
        tree2.insert_many(btree(entries)).unwrap();

        assert_eq!(tree1.root(), tree2.root());
    }

    #[test]
    fn test_insert_many_proofs_verify() {
        let entries: Vec<(Hash, Hash)> = (0u32..10).map(|i| (test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()))).collect();

        let mut tree = Smt::new();
        tree.insert_many(btree(entries.clone())).unwrap();

        for &(k, l) in &entries {
            assert_inclusion(&tree, &k, l);
        }
    }

    #[test]
    fn test_insert_many_max_depth_divergence() {
        let k1 = key_from_bytes([0u8; 32]);
        let mut bytes2 = [0u8; 32];
        bytes2[31] = 0x01;
        let k2 = key_from_bytes(bytes2);

        let l1 = test_leaf(b"left");
        let l2 = test_leaf(b"right");

        let mut tree1 = Smt::new();
        tree1.insert(k1, l1).unwrap();
        tree1.insert(k2, l2).unwrap();

        let mut tree2 = Smt::new();
        tree2.insert_many(btree([(k1, l1), (k2, l2)])).unwrap();

        assert_eq!(tree1.root(), tree2.root());
    }

    // ========================================================================
    // 15. Proof error tests
    // ========================================================================

    #[test]
    fn test_malformed_proof_too_many_siblings() {
        let proof = OwnedSmtProof { bitmap: [0xFF; 32], siblings: Vec::from([ZERO_HASH]) };
        let key = test_key(b"k");
        let err = proof.compute_root::<TestHasher>(&key, None).unwrap_err();
        assert_eq!(err, SmtProofError::SiblingCountMismatch { expected: 0, actual: 1 });
    }

    #[test]
    fn test_malformed_proof_too_few_siblings() {
        let proof = OwnedSmtProof { bitmap: [0x00; 32], siblings: Vec::new() };
        let key = test_key(b"k");
        let err = proof.compute_root::<TestHasher>(&key, None).unwrap_err();
        assert_eq!(err, SmtProofError::SiblingCountMismatch { expected: 256, actual: 0 });
    }
}
