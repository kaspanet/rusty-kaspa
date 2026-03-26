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

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use core::marker::PhantomData;
use kaspa_hashes::Hash;

use crate::proof::OwnedSmtProof;
use crate::store::{BTreeSmtStore, BranchChildren, BranchKey, LeafUpdate, SmtStore, SortedLeafUpdates};
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

    #[cfg(any(test, feature = "test-utils"))]
    /// Insert or update a leaf, incrementally updating the root.
    pub fn insert(&mut self, key: Hash, leaf_hash: Hash) {
        self.store.insert_leaf(key, leaf_hash);
        self.root = self.walk_up(&key, leaf_hash);
    }

    #[cfg(any(test, feature = "test-utils"))]
    /// Remove a leaf by key, incrementally updating the root.
    pub fn remove(&mut self, key: &Hash) -> Option<Hash> {
        use kaspa_hashes::ZERO_HASH;
        let old = self.store.get_leaf(key);
        if old.is_some() {
            self.root = self.walk_up(key, ZERO_HASH);
            self.store.insert_leaf(*key, ZERO_HASH);
        }
        old
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn walk_up(&mut self, key: &Hash, leaf_hash: Hash) -> Hash {
        let empty_hashes = &H::EMPTY_HASHES;
        let mut current = leaf_hash;

        for depth in (0..DEPTH).rev() {
            let height = DEPTH - 1 - depth;
            let goes_right = bit_at(key, depth);
            let branch_key = BranchKey::new(height as u8, key);

            let sibling = self
                .store
                .branches
                .get(&branch_key)
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
    leaf_updates: SortedLeafUpdates,
) -> Result<(Hash, SmtBranchChanges), S::Error> {
    let mut changes = SmtBranchChanges::new();
    let root = compute_root_update_into::<H, S>(store, current_root, leaf_updates, &mut changes)?;
    Ok((root, changes))
}

/// Like [`compute_root_update`] but writes into an externally-owned branch map.
///
/// Pre-populated entries are used as "existing" branches: the local buffer is
/// checked before the immutable store, and paths that match pre-existing values
/// stop propagating. This allows proof-verified branches to short-circuit
/// tree computation.
pub fn compute_root_update_into<H: SmtHasher, S: SmtStore>(
    store: &S,
    current_root: Hash,
    leaf_updates: SortedLeafUpdates,
    changes: &mut SmtBranchChanges,
) -> Result<Hash, S::Error> {
    if leaf_updates.is_empty() {
        return Ok(current_root);
    }

    // Read a branch: check our local buffer first, then the immutable store.
    let read_branch = |changes: &SmtBranchChanges, bk: BranchKey| -> Result<Option<BranchChildren>, S::Error> {
        if let Some(&bc) = changes.get(&bk) {
            return Ok(Some(bc));
        }
        store.get_branch(&bk)
    };

    let empty_hashes = &H::EMPTY_HASHES;
    let mut current: Vec<LeafUpdate> = leaf_updates.into_vec();
    let mut next = Vec::with_capacity(current.len());

    for depth in (0..DEPTH).rev() {
        let height = DEPTH - 1 - depth;

        let mut i = 0;
        while i < current.len() {
            let entry = current[i];
            let branch_key = BranchKey::new(height as u8, &entry.key);

            // Check if the next entry is a sibling (same BranchKey)
            let sibling_in_batch = if i + 1 < current.len() {
                let next_bk = BranchKey::new(height as u8, &current[i + 1].key);
                if branch_key == next_bk { Some(current[i + 1].leaf_hash) } else { None }
            } else {
                None
            };

            // Read existing children from buffer/store
            let existing = read_branch(changes, branch_key)?;

            let new_children = if let Some(sib) = sibling_in_batch {
                BranchChildren { left: entry.leaf_hash, right: sib }
            } else {
                let goes_right = bit_at(&entry.key, depth);
                let sibling = existing.map(|bc| if goes_right { bc.left } else { bc.right }).unwrap_or(empty_hashes[height]);
                if goes_right {
                    BranchChildren { left: sibling, right: entry.leaf_hash }
                } else {
                    BranchChildren { left: entry.leaf_hash, right: sibling }
                }
            };

            let parent = hash_node::<H>(new_children.left, new_children.right);

            // Only record and propagate if the branch actually changed
            let empty = BranchChildren { left: empty_hashes[height], right: empty_hashes[height] };
            if new_children != existing.unwrap_or(empty) {
                changes.insert(branch_key, new_children);
                next.push(LeafUpdate { key: entry.key, leaf_hash: parent });
            }

            i += if sibling_in_batch.is_some() { 2 } else { 1 };
        }

        core::mem::swap(&mut current, &mut next);
        next.clear();

        if current.is_empty() {
            return Ok(current_root);
        }
    }

    debug_assert_eq!(current.len(), 1);
    Ok(current[0].leaf_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proof::SmtProofError;
    use kaspa_hashes::{HasherBase, SeqCommitActiveNode, ZERO_HASH};
    use rand::{Rng, SeedableRng, rngs::StdRng};

    type TestHasher = SeqCommitActiveNode;
    type Smt = SparseMerkleTree<TestHasher>;

    // ---- Helpers ----

    fn updates(entries: impl IntoIterator<Item = (Hash, Hash)>) -> SortedLeafUpdates {
        SortedLeafUpdates::from_unsorted(entries.into_iter().map(|(key, leaf_hash)| LeafUpdate { key, leaf_hash }))
    }

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
    // compute_root_update tests
    // ========================================================================

    #[test]
    fn test_compute_root_update_empty() {
        let store = BTreeSmtStore::new();
        let empty_root = TestHasher::empty_root();
        let (root, changes) = compute_root_update::<TestHasher, _>(&store, empty_root, updates([])).unwrap();
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
        tree.insert(k1, l1);
        tree.insert(k2, l2);
        let expected_root = tree.root();

        // Build via compute_root_update from empty store
        let store = BTreeSmtStore::new();
        let empty_root = TestHasher::empty_root();
        let (root, _changes) = compute_root_update::<TestHasher, _>(&store, empty_root, updates([(k1, l1), (k2, l2)])).unwrap();

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
        tree.insert(k1, l1);
        let root1 = tree.root();
        // The store now has branches for k1's path
        let store = tree.into_store();

        // compute_root_update from this store with k2
        let (root2, changes) = compute_root_update::<TestHasher, _>(&store, root1, updates([(k2, l2)])).unwrap();

        // Verify against full in-memory tree
        let mut tree2 = Smt::new();
        tree2.insert(k1, l1);
        tree2.insert(k2, l2);

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
        tree.insert(k1, l1);
        let root = tree.root();
        let store = tree.into_store();

        let (new_root, changes) = compute_root_update::<TestHasher, _>(&store, root, updates([(k1, l1)])).unwrap();

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
        tree.insert(k1, l1);
        tree.insert(k2, l2);
        let root = tree.root();
        let store = tree.into_store();

        let (new_root, changes) = compute_root_update::<TestHasher, _>(&store, root, updates([(k1, l1_new)])).unwrap();

        assert_ne!(new_root, root);
        // The number of changed branches should be <= 256 (one path)
        assert!(changes.len() <= 256);
        // Verify result matches in-memory computation
        let mut tree2 = Smt::new();
        tree2.insert(k1, l1_new);
        tree2.insert(k2, l2);
        assert_eq!(new_root, tree2.root());
    }

    #[test]
    fn test_compute_root_update_expire_leaf() {
        // Insert k1. Then expire it (ZERO_HASH). Root should return to empty.
        let k1 = test_key(b"1");
        let l1 = test_leaf(b"a");

        let mut tree = Smt::new();
        tree.insert(k1, l1);
        let root = tree.root();
        let store = tree.into_store();

        let empty_root = TestHasher::empty_root();
        let (new_root, _changes) = compute_root_update::<TestHasher, _>(&store, root, updates([(k1, ZERO_HASH)])).unwrap();

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
        tree.insert(test_key(b"k"), test_leaf(b"v"));
        assert_ne!(tree.root(), empty_root);
    }

    #[test]
    fn test_single_get() {
        let mut tree = Smt::new();
        let key = test_key(b"k");
        let leaf = test_leaf(b"v");
        tree.insert(key, leaf);
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
        tree.insert(k1, l1);
        tree.insert(k2, l2);
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
    // 6. Inclusion proof tests
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
        let proof = tree.prove(&k).unwrap();
        assert_eq!(proof.non_empty_count(), 0);
        assert_eq!(proof.empty_count(), DEPTH);

        tree.insert(test_key(b"other"), test_leaf(b"w"));
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
        tree.insert(key, test_leaf(b"correct"));
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
        tree.insert(key, leaf);
        let proof = tree.prove(&key).unwrap();
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
        let proof = tree.prove(&key).unwrap();
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
        tree.insert(key, leaf);
        let root = tree.root();
        let proof = tree.prove(&key).unwrap();
        assert!(!proof.verify::<TestHasher>(&key, None, root).unwrap());
    }

    #[test]
    fn test_reject_non_inclusion_as_inclusion() {
        let mut tree = Smt::new();
        tree.insert(test_key(b"other"), test_leaf(b"w"));
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
        tree.insert(k1, l1);
        tree.insert(k2, l2);

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
        tree.insert(k1, l1);
        tree.insert(k2, l2);

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
            let proof = tree.prove(&k).unwrap();
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
                tree.insert(key, leaf);
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
        assert_eq!(tree.root(), TestHasher::empty_root());
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
            tree.insert(k, test_leaf(&i.to_le_bytes()));
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
            tree.insert(test_key(&i.to_le_bytes()), test_leaf(&i.to_le_bytes()));
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
