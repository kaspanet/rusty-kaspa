//! Sparse Merkle Tree with Suffix-Only Leaf (SLO) optimization.
//!
//! # Two APIs
//!
//! - **In-memory** (`SparseMerkleTree<H, BTreeSmtStore>`): mutable `insert`/`remove`/`prove`.
//!   Used in tests and for RPC proof generation.
//!
//! - **Pure computation** ([`compute_root_update`]): top-down recursive algorithm that
//!   reads from an immutable `&impl SmtStore` and returns `(new_root, changed_nodes)`.
//!   Single-leaf subtrees are collapsed into one node instead of 256 branches.
//!   Used by consensus (`SmtProcessor::build`).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use core::marker::PhantomData;
use kaspa_hashes::Hash;

use crate::proof::OwnedSmtProof;
use crate::store::{BTreeSmtStore, BranchChildren, BranchKey, CollapsedLeaf, LeafUpdate, Node, SmtStore, SortedLeafUpdates};
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

            let sibling = match self.store.get_node(&branch_key)? {
                Some(Node::Internal(bc)) => {
                    if goes_right {
                        bc.left
                    } else {
                        bc.right
                    }
                }
                _ => empty_hashes[height],
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

            let sibling = match self.store.get_node(&branch_key).unwrap() {
                Some(Node::Internal(bc)) => {
                    if goes_right {
                        bc.left
                    } else {
                        bc.right
                    }
                }
                _ => empty_hashes[height],
            };

            let (left, right) = if goes_right { (sibling, current) } else { (current, sibling) };
            let parent_hash = hash_node::<H>(left, right);
            self.store.insert_node(branch_key, Node::Internal(BranchChildren { left, right }));
            current = parent_hash;
        }

        current
    }
}

/// Map of changed nodes: `BranchKey → Node`.
///
/// A tombstone entry (`Node::Internal` with both children `ZERO_HASH`)
/// means "this node was deleted." Stores should remove the entry rather than
/// persist the tombstone value.
pub type SmtNodeChanges = BTreeMap<BranchKey, Node>;

/// Tombstone sentinel: signals that a node was deleted. Not a valid tree node.
const TOMBSTONE: Node = Node::Internal(BranchChildren { left: kaspa_hashes::ZERO_HASH, right: kaspa_hashes::ZERO_HASH });

/// Result of computing a subtree — propagated upward during recursion.
enum NodeResult {
    /// Subtree is empty (no leaves).
    Empty,
    /// Subtree contains exactly one leaf (collapsed).
    Collapsed(CollapsedLeaf),
    /// Subtree contains two or more leaves (standard internal).
    Internal { hash: Hash },
}

impl NodeResult {
    /// Compute the hash this node contributes to its parent at the given child `height`.
    fn hash<H: SmtHasher>(&self, height: usize) -> Hash {
        match self {
            NodeResult::Empty => H::EMPTY_HASHES[height],
            NodeResult::Collapsed(cl) => hash_node::<H::CollapsedHasher>(cl.lane_key, cl.leaf_hash),
            NodeResult::Internal { hash } => *hash,
        }
    }
}

/// Compute the new SMT root and **only the changed nodes** for a set of leaf updates.
///
/// Uses top-down recursion with Suffix-Only Leaf (SLO) optimization:
/// collapsed single-leaf subtrees are represented as a single node instead of
/// a full 256-level branch chain, cutting writes from O(256) to O(1) per lone leaf.
///
/// Reads from an immutable `&impl SmtStore`. Does not mutate any state.
/// Returns `(new_root, changed_nodes)`.
pub fn compute_root_update<H: SmtHasher, S: SmtStore>(
    store: &S,
    current_root: Hash,
    leaf_updates: SortedLeafUpdates,
) -> Result<(Hash, SmtNodeChanges), S::Error> {
    let mut changes = SmtNodeChanges::new();
    let root = compute_root_update_into::<H, S>(store, current_root, leaf_updates, &mut changes)?;
    Ok((root, changes))
}

/// Like [`compute_root_update`] but writes into an externally-owned node map.
///
/// Pre-populated entries act as a proof cache: the local buffer is checked
/// before the immutable store.
pub fn compute_root_update_into<H: SmtHasher, S: SmtStore>(
    store: &S,
    current_root: Hash,
    leaf_updates: SortedLeafUpdates,
    changes: &mut SmtNodeChanges,
) -> Result<Hash, S::Error> {
    if leaf_updates.is_empty() {
        return Ok(current_root);
    }

    let updates = leaf_updates.into_vec();
    let result = compute_subtree::<H, S>(store, changes, &updates, 0)?;

    // Convert the root-level NodeResult to a root hash
    match &result {
        NodeResult::Empty => Ok(H::empty_root()),
        NodeResult::Collapsed(cl) => Ok(hash_node::<H::CollapsedHasher>(cl.lane_key, cl.leaf_hash)),
        NodeResult::Internal { hash } => Ok(*hash),
    }
}

/// Read a node from the changes map first, then the store.
/// Tombstone entries are treated as absent (deleted).
fn read_node<S: SmtStore>(store: &S, changes: &SmtNodeChanges, bk: &BranchKey) -> Result<Option<Node>, S::Error> {
    if let Some(&nk) = changes.get(bk) {
        return Ok(if nk == TOMBSTONE { None } else { Some(nk) });
    }
    store.get_node(bk)
}

/// Compute the BranchKey for a child of `parent` on the given side.
fn child_branch_key(parent: &BranchKey, right: bool, depth: usize) -> BranchKey {
    let child_height = parent.height - 1;
    let mut bytes = parent.node_key.as_bytes();
    if right {
        bytes[depth / 8] |= 0x80 >> (depth % 8);
    }
    BranchKey { height: child_height, node_key: Hash::from_bytes(bytes) }
}

/// Recursive top-down subtree computation.
///
/// `updates` must be sorted by key and non-empty.
/// `depth` is the current tree depth (0 = root, 255 = leaf parent).
fn compute_subtree<H: SmtHasher, S: SmtStore>(
    store: &S,
    changes: &mut SmtNodeChanges,
    updates: &[LeafUpdate],
    depth: usize,
) -> Result<NodeResult, S::Error> {
    debug_assert!(!updates.is_empty());
    let height = DEPTH - 1 - depth;
    let subtree_key = BranchKey::new(height as u8, &updates[0].key);

    // Leaf level: no node to write, just return the leaf state
    if depth == DEPTH - 1 {
        debug_assert_eq!(updates.len(), 1);
        let u = &updates[0];
        return Ok(if u.leaf_hash == kaspa_hashes::ZERO_HASH {
            NodeResult::Empty
        } else {
            NodeResult::Collapsed(CollapsedLeaf { lane_key: u.key, leaf_hash: u.leaf_hash })
        });
    }

    let existing = read_node::<S>(store, changes, &subtree_key)?;

    // Single update into empty subtree: immediate collapse

    if updates.len() == 1 && existing.is_none() {
        let u = &updates[0];
        if u.leaf_hash == kaspa_hashes::ZERO_HASH {
            return Ok(NodeResult::Empty); // Deleting from empty — no-op
        }
        let cl = CollapsedLeaf { lane_key: u.key, leaf_hash: u.leaf_hash };
        changes.insert(subtree_key, Node::Collapsed(cl));
        return Ok(NodeResult::Collapsed(cl));
    }

    // Same-key update of a collapsed node

    if updates.len() == 1
        && let Some(Node::Collapsed(existing_cl)) = existing
        && updates[0].key == existing_cl.lane_key
    {
        let u = &updates[0];
        if u.leaf_hash == kaspa_hashes::ZERO_HASH {
            changes.insert(subtree_key, TOMBSTONE);
            return Ok(NodeResult::Empty);
        }
        let cl = CollapsedLeaf { lane_key: u.key, leaf_hash: u.leaf_hash };
        let node = Node::Collapsed(cl);
        if existing != Some(node) {
            changes.insert(subtree_key, node);
        }
        return Ok(NodeResult::Collapsed(cl));
    }

    // Expand collapsed node: inject existing leaf as phantom update

    let effective_buf: Vec<LeafUpdate>;
    let (updates, existing_is_expanded) = if let Some(Node::Collapsed(cl)) = existing {
        let mut v = Vec::with_capacity(updates.len() + 1);
        v.extend_from_slice(updates);
        if !updates.iter().any(|u| u.key == cl.lane_key) {
            v.push(LeafUpdate { key: cl.lane_key, leaf_hash: cl.leaf_hash });
        }
        v.sort_unstable_by_key(|u| u.key);
        effective_buf = v;
        (&effective_buf[..], true)
    } else {
        (updates, false)
    };

    // Split and recurse

    let split_pos = updates.partition_point(|u| !bit_at(&u.key, depth));
    let left_updates = &updates[..split_pos];
    let right_updates = &updates[split_pos..];

    let left_result = if left_updates.is_empty() {
        read_child_result::<H, S>(store, changes, &subtree_key, false, depth)?
    } else {
        compute_subtree::<H, S>(store, changes, left_updates, depth + 1)?
    };

    let right_result = if right_updates.is_empty() {
        read_child_result::<H, S>(store, changes, &subtree_key, true, depth)?
    } else {
        compute_subtree::<H, S>(store, changes, right_updates, depth + 1)?
    };

    // Merge and write

    let child_height = height - 1;
    let left_hash = left_result.hash::<H>(child_height);
    let right_hash = right_result.hash::<H>(child_height);

    let result = match (&left_result, &right_result) {
        (NodeResult::Empty, NodeResult::Empty) => NodeResult::Empty,
        (NodeResult::Collapsed(cl), NodeResult::Empty) | (NodeResult::Empty, NodeResult::Collapsed(cl)) => NodeResult::Collapsed(*cl),
        _ => {
            let hash = hash_node::<H>(left_hash, right_hash);
            NodeResult::Internal { hash }
        }
    };

    // Write to changes map (None when expanded, since the old Collapsed is being replaced)
    let existing_for_write = if existing_is_expanded { None } else { existing };
    match &result {
        NodeResult::Empty => {
            if existing_for_write.is_some() || existing_is_expanded {
                changes.insert(subtree_key, TOMBSTONE);
            }
        }
        NodeResult::Collapsed(cl) => {
            let node = Node::Collapsed(*cl);
            if existing_for_write != Some(node) {
                changes.insert(subtree_key, node);
            }
        }
        NodeResult::Internal { .. } => {
            let node = Node::Internal(BranchChildren { left: left_hash, right: right_hash });
            if existing_for_write != Some(node) {
                changes.insert(subtree_key, node);
            }
        }
    }

    Ok(result)
}

/// Read the existing child result on the side that has no updates.
///
/// Reads the actual child node from the store to accurately determine
/// whether it's Collapsed (enabling collapse-upward) or Internal.
fn read_child_result<H: SmtHasher, S: SmtStore>(
    store: &S,
    changes: &SmtNodeChanges,
    parent_key: &BranchKey,
    right: bool,
    depth: usize,
) -> Result<NodeResult, S::Error> {
    let existing = read_node::<S>(store, changes, parent_key)?;
    match existing {
        None => Ok(NodeResult::Empty),
        Some(Node::Collapsed(cl)) => {
            if bit_at(&cl.lane_key, depth) == right {
                Ok(NodeResult::Collapsed(cl))
            } else {
                Ok(NodeResult::Empty)
            }
        }
        Some(Node::Internal(bc)) => {
            let child_hash = if right { bc.right } else { bc.left };
            let child_height = parent_key.height - 1;
            if child_hash == H::EMPTY_HASHES[child_height as usize] {
                return Ok(NodeResult::Empty);
            }
            let child_key = child_branch_key(parent_key, right, depth);
            match read_node::<S>(store, changes, &child_key)? {
                Some(Node::Collapsed(cl)) => Ok(NodeResult::Collapsed(cl)),
                _ => Ok(NodeResult::Internal { hash: child_hash }),
            }
        }
    }
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

    /// Helper: build SLO store from changes
    fn apply_changes(store: &mut BTreeSmtStore, changes: &SmtNodeChanges) {
        for (bk, node) in changes {
            store.insert_node(*bk, *node);
        }
    }

    #[test]
    fn test_compute_root_update_batch_matches_incremental() {
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");

        // Batch insert
        let store = BTreeSmtStore::new();
        let empty_root = TestHasher::empty_root();
        let (batch_root, _) = compute_root_update::<TestHasher, _>(&store, empty_root, updates([(k1, l1), (k2, l2)])).unwrap();

        // Incremental: insert k1 then k2
        let mut store2 = BTreeSmtStore::new();
        let (root1, changes1) = compute_root_update::<TestHasher, _>(&store2, empty_root, updates([(k1, l1)])).unwrap();
        apply_changes(&mut store2, &changes1);
        let (root2, _) = compute_root_update::<TestHasher, _>(&store2, root1, updates([(k2, l2)])).unwrap();

        assert_eq!(batch_root, root2);
        assert_ne!(batch_root, empty_root);
    }

    #[test]
    fn test_compute_root_update_incremental() {
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");

        // Insert k1, persist to store
        let mut store = BTreeSmtStore::new();
        let empty_root = TestHasher::empty_root();
        let (root1, changes1) = compute_root_update::<TestHasher, _>(&store, empty_root, updates([(k1, l1)])).unwrap();
        apply_changes(&mut store, &changes1);

        // Add k2 incrementally
        let (root2, changes2) = compute_root_update::<TestHasher, _>(&store, root1, updates([(k2, l2)])).unwrap();

        assert_ne!(root2, root1);
        // SLO: changes should be compact (collapsed nodes + internal at divergence)
        assert!(!changes2.is_empty());
    }

    #[test]
    fn test_compute_root_update_unchanged_no_propagation() {
        let k1 = test_key(b"1");
        let l1 = test_leaf(b"a");

        // Insert k1 via SLO
        let mut store = BTreeSmtStore::new();
        let empty_root = TestHasher::empty_root();
        let (root, changes) = compute_root_update::<TestHasher, _>(&store, empty_root, updates([(k1, l1)])).unwrap();
        apply_changes(&mut store, &changes);

        // "Update" k1 with same value — no changes
        let (new_root, new_changes) = compute_root_update::<TestHasher, _>(&store, root, updates([(k1, l1)])).unwrap();

        assert_eq!(new_root, root, "same leaf value should produce same root");
        assert!(new_changes.is_empty(), "no nodes should change when leaf value is identical");
    }

    #[test]
    fn test_compute_root_update_only_changed_branches() {
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");
        let l1_new = test_leaf(b"a_new");

        // Insert k1 and k2
        let mut store = BTreeSmtStore::new();
        let empty_root = TestHasher::empty_root();
        let (root, changes) = compute_root_update::<TestHasher, _>(&store, empty_root, updates([(k1, l1), (k2, l2)])).unwrap();
        apply_changes(&mut store, &changes);

        // Update only k1
        let (new_root, _new_changes) = compute_root_update::<TestHasher, _>(&store, root, updates([(k1, l1_new)])).unwrap();

        assert_ne!(new_root, root);
        // Verify: batch from scratch matches
        let store3 = BTreeSmtStore::new();
        let (expected, _) = compute_root_update::<TestHasher, _>(&store3, empty_root, updates([(k1, l1_new), (k2, l2)])).unwrap();
        assert_eq!(new_root, expected);
    }

    #[test]
    fn test_compute_root_update_expire_leaf() {
        let k1 = test_key(b"1");
        let l1 = test_leaf(b"a");

        // Insert k1 via SLO
        let mut store = BTreeSmtStore::new();
        let empty_root = TestHasher::empty_root();
        let (root, changes) = compute_root_update::<TestHasher, _>(&store, empty_root, updates([(k1, l1)])).unwrap();
        apply_changes(&mut store, &changes);

        // Expire k1
        let (new_root, _) = compute_root_update::<TestHasher, _>(&store, root, updates([(k1, ZERO_HASH)])).unwrap();

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

    // ========================================================================
    // SLO (Suffix-Only Leaf) top-down algorithm tests
    // ========================================================================

    #[test]
    fn test_slo_single_leaf_collapsed() {
        let k1 = test_key(b"1");
        let l1 = test_leaf(b"a");

        let store = BTreeSmtStore::new();
        let (root, changes) = compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates([(k1, l1)])).unwrap();

        // Should produce a single Collapsed entry at the root level (height 255)
        assert_eq!(changes.len(), 1, "single leaf should produce one collapsed node");
        let (&bk, &nk) = changes.iter().next().unwrap();
        assert_eq!(bk.height, 255);
        assert!(matches!(nk, Node::Collapsed(cl) if cl.lane_key == k1 && cl.leaf_hash == l1));

        // Root should not be the empty root
        assert_ne!(root, TestHasher::empty_root());
    }

    #[test]
    fn test_slo_two_leaves_internal() {
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");

        let store = BTreeSmtStore::new();
        let (root, changes) =
            compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates([(k1, l1), (k2, l2)])).unwrap();

        assert_ne!(root, TestHasher::empty_root());

        // There should be collapsed nodes for each leaf and internal nodes above
        let collapsed_count = changes.values().filter(|n| matches!(n, Node::Collapsed(_))).count();
        let internal_count = changes.values().filter(|n| matches!(n, Node::Internal(_))).count();
        assert_eq!(collapsed_count, 2, "two leaves should produce two collapsed nodes");
        assert!(internal_count >= 1, "at least one internal node above the divergence point");
    }

    #[test]
    fn test_slo_expire_sole_leaf() {
        let k1 = test_key(b"1");
        let l1 = test_leaf(b"a");

        // Insert one leaf
        let store = BTreeSmtStore::new();
        let (root1, changes1) = compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates([(k1, l1)])).unwrap();
        assert_ne!(root1, TestHasher::empty_root());

        // Apply changes to store
        let mut store2 = BTreeSmtStore::new();
        for (bk, nk) in &changes1 {
            store2.insert_node(*bk, *nk);
        }

        // Expire the leaf
        let (root2, _changes2) = compute_root_update::<TestHasher, _>(&store2, root1, updates([(k1, ZERO_HASH)])).unwrap();
        assert_eq!(root2, TestHasher::empty_root(), "expiring sole leaf should return to empty root");
    }

    #[test]
    fn test_slo_expire_one_of_two() {
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");

        // Insert two leaves
        let store = BTreeSmtStore::new();
        let (root1, changes1) =
            compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates([(k1, l1), (k2, l2)])).unwrap();

        // Apply to store
        let mut store2 = BTreeSmtStore::new();
        for (bk, nk) in &changes1 {
            store2.insert_node(*bk, *nk);
        }

        // Expire k1 — k2 should collapse upward
        let (root2, changes2) = compute_root_update::<TestHasher, _>(&store2, root1, updates([(k1, ZERO_HASH)])).unwrap();

        // After expiring one of two, we should be back to a single-leaf tree
        // The remaining leaf should be represented as a single collapsed node at root
        let collapsed_count = changes2.values().filter(|n| matches!(n, Node::Collapsed(_))).count();
        assert!(collapsed_count >= 1, "surviving leaf should collapse upward");

        // Root should match inserting just k2 from scratch
        let store3 = BTreeSmtStore::new();
        let (root_k2_only, _) = compute_root_update::<TestHasher, _>(&store3, TestHasher::empty_root(), updates([(k2, l2)])).unwrap();
        assert_eq!(root2, root_k2_only, "expiring k1 should produce same root as only inserting k2");
    }

    #[test]
    fn test_slo_expand_collapsed() {
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");

        // Insert one leaf (collapsed)
        let store = BTreeSmtStore::new();
        let (root1, changes1) = compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates([(k1, l1)])).unwrap();

        let mut store2 = BTreeSmtStore::new();
        for (bk, nk) in &changes1 {
            store2.insert_node(*bk, *nk);
        }

        // Insert second leaf — should expand the collapsed node
        let (root2, changes2) = compute_root_update::<TestHasher, _>(&store2, root1, updates([(k2, l2)])).unwrap();

        // Should have two collapsed nodes and at least one internal
        let collapsed_count = changes2.values().filter(|n| matches!(n, Node::Collapsed(_))).count();
        assert!(collapsed_count >= 1, "expansion should produce collapsed nodes");

        // Root should match inserting both from scratch
        let store3 = BTreeSmtStore::new();
        let (root_both, _) =
            compute_root_update::<TestHasher, _>(&store3, TestHasher::empty_root(), updates([(k1, l1), (k2, l2)])).unwrap();
        assert_eq!(root2, root_both, "incremental expand should match batch insert");
    }

    #[test]
    fn test_slo_same_key_update() {
        let k1 = test_key(b"1");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");

        // Insert k1 with l1
        let store = BTreeSmtStore::new();
        let (root1, changes1) = compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates([(k1, l1)])).unwrap();

        let mut store2 = BTreeSmtStore::new();
        for (bk, nk) in &changes1 {
            store2.insert_node(*bk, *nk);
        }

        // Update k1 with l2 (same key, different value — no expansion)
        let (root2, changes2) = compute_root_update::<TestHasher, _>(&store2, root1, updates([(k1, l2)])).unwrap();

        assert_ne!(root2, root1, "updating leaf value should change root");

        // Should still be a single collapsed node
        let collapsed_count = changes2.values().filter(|n| matches!(n, Node::Collapsed(_))).count();
        assert_eq!(collapsed_count, 1, "same-key update should remain collapsed");

        // Match inserting k1 with l2 from scratch
        let store3 = BTreeSmtStore::new();
        let (root_scratch, _) = compute_root_update::<TestHasher, _>(&store3, TestHasher::empty_root(), updates([(k1, l2)])).unwrap();
        assert_eq!(root2, root_scratch);
    }

    #[test]
    fn test_slo_batch_mixed_insert_expire() {
        let k1 = test_key(b"1");
        let k2 = test_key(b"2");
        let k3 = test_key(b"3");
        let l1 = test_leaf(b"a");
        let l2 = test_leaf(b"b");
        let l3 = test_leaf(b"c");

        // Insert k1, k2
        let store = BTreeSmtStore::new();
        let (root1, changes1) =
            compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates([(k1, l1), (k2, l2)])).unwrap();

        let mut store2 = BTreeSmtStore::new();
        for (bk, nk) in &changes1 {
            store2.insert_node(*bk, *nk);
        }

        // Batch: expire k1, insert k3
        let (root2, _changes2) = compute_root_update::<TestHasher, _>(&store2, root1, updates([(k1, ZERO_HASH), (k3, l3)])).unwrap();

        // Should match inserting k2, k3 from scratch
        let store3 = BTreeSmtStore::new();
        let (root_expected, _) =
            compute_root_update::<TestHasher, _>(&store3, TestHasher::empty_root(), updates([(k2, l2), (k3, l3)])).unwrap();
        assert_eq!(root2, root_expected);
    }

    #[test]
    fn test_slo_deep_shared_prefix() {
        // Create two keys that share a long prefix and diverge late
        let mut bytes1 = [0u8; 32];
        let mut bytes2 = [0u8; 32];
        // Same first 31 bytes, differ only in last byte
        bytes1[31] = 0b00000010; // bit 254 = 1
        bytes2[31] = 0b00000001; // bit 255 = 1
        let k1 = key_from_bytes(bytes1);
        let k2 = key_from_bytes(bytes2);
        let l1 = test_leaf(b"deep1");
        let l2 = test_leaf(b"deep2");

        let store = BTreeSmtStore::new();
        let (root, changes) =
            compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates([(k1, l1), (k2, l2)])).unwrap();

        // The two keys diverge very late (near leaf level), so we should have
        // internal nodes from the divergence point to the root, but collapsed below
        assert_ne!(root, TestHasher::empty_root());
        assert!(changes.len() >= 3, "deep prefix sharing: 2 collapsed + internal at divergence");
    }

    #[test]
    fn test_slo_collapse_propagates_levels() {
        // Insert 3 keys, expire 2 of them, verify the surviving one collapses all the way up
        let k1 = test_key(b"c1");
        let k2 = test_key(b"c2");
        let k3 = test_key(b"c3");
        let l1 = test_leaf(b"v1");
        let l2 = test_leaf(b"v2");
        let l3 = test_leaf(b"v3");

        let store = BTreeSmtStore::new();
        let (root1, changes1) =
            compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates([(k1, l1), (k2, l2), (k3, l3)])).unwrap();

        let mut store2 = BTreeSmtStore::new();
        for (bk, nk) in &changes1 {
            store2.insert_node(*bk, *nk);
        }

        // Expire k1 and k2
        let (root2, _) = compute_root_update::<TestHasher, _>(&store2, root1, updates([(k1, ZERO_HASH), (k2, ZERO_HASH)])).unwrap();

        // Should match k3-only tree
        let store3 = BTreeSmtStore::new();
        let (root_k3_only, _) = compute_root_update::<TestHasher, _>(&store3, TestHasher::empty_root(), updates([(k3, l3)])).unwrap();
        assert_eq!(root2, root_k3_only, "after expiring 2 of 3, surviving leaf should collapse to root");
    }

    #[test]
    fn test_slo_stress_random() {
        let mut rng = StdRng::seed_from_u64(42);
        let n = 100;

        // Generate random keys and leaf hashes
        let mut keys = Vec::new();
        let mut leaves = Vec::new();
        for _ in 0..n {
            let mut key_bytes = [0u8; 32];
            rng.fill(&mut key_bytes);
            keys.push(Hash::from_bytes(key_bytes));
            let mut leaf_bytes = [0u8; 32];
            rng.fill(&mut leaf_bytes);
            leaves.push(Hash::from_bytes(leaf_bytes));
        }

        // Insert all at once (batch)
        let all_updates: Vec<(Hash, Hash)> = keys.iter().copied().zip(leaves.iter().copied()).collect();
        let store = BTreeSmtStore::new();
        let (root_batch, _) =
            compute_root_update::<TestHasher, _>(&store, TestHasher::empty_root(), updates(all_updates.clone())).unwrap();

        // Insert one by one (incremental)
        let mut store_incr = BTreeSmtStore::new();
        let mut root_incr = TestHasher::empty_root();
        for &(k, l) in &all_updates {
            let (new_root, changes) = compute_root_update::<TestHasher, _>(&store_incr, root_incr, updates([(k, l)])).unwrap();
            for (bk, nk) in &changes {
                store_incr.insert_node(*bk, *nk);
            }
            root_incr = new_root;
        }

        assert_eq!(root_batch, root_incr, "batch and incremental should produce identical roots");

        // Expire half randomly
        let to_expire: Vec<(Hash, Hash)> = keys[..n / 2].iter().map(|k| (*k, ZERO_HASH)).collect();
        let remaining: Vec<(Hash, Hash)> = keys[n / 2..].iter().copied().zip(leaves[n / 2..].iter().copied()).collect();

        let (root_after_expire, changes_expire) =
            compute_root_update::<TestHasher, _>(&store_incr, root_incr, updates(to_expire)).unwrap();

        // Apply expire changes
        let mut store_remaining = store_incr;
        for (bk, nk) in &changes_expire {
            store_remaining.insert_node(*bk, *nk);
        }

        // Insert remaining from scratch
        let store_fresh = BTreeSmtStore::new();
        let (root_fresh, _) =
            compute_root_update::<TestHasher, _>(&store_fresh, TestHasher::empty_root(), updates(remaining)).unwrap();

        assert_eq!(root_after_expire, root_fresh, "expire half should match inserting only the remaining half");
    }
}
