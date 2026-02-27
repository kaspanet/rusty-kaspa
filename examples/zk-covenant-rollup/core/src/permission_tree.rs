//! Permission (exit) Merkle tree for L2→L1 withdrawals.
//!
//! A SHA2-256 Merkle tree of `(spk, amount)` leaves. Built fresh per proof
//! from accumulated exit actions. The root is committed to the journal
//! so the on-chain script can verify individual withdrawal claims.
//!
//! Tree depth is variable based on leaf count: `depth = ceil(log2(count))`,
//! max depth 8 (256 leaves).

#[cfg(feature = "std")]
use alloc::{vec, vec::Vec};
use sha2::Digest;

use crate::streaming_merkle::{MerkleHashOps, StreamingMerkle};

/// Maximum tree depth (256 leaves)
pub const PERM_MAX_DEPTH: usize = 8;

/// Maximum SPK size in bytes (P2SH = 35 bytes, padded to 36 for alignment)
pub const MAX_SPK_SIZE: usize = 36;

/// Domain prefix for permission leaf hashing
const PERM_LEAF_DOMAIN: &[u8; 8] = b"PermLeaf";

/// Domain prefix for empty permission leaf
const PERM_EMPTY_DOMAIN: &[u8; 9] = b"PermEmpty";

/// Domain prefix for permission branch hashing
const PERM_BRANCH_DOMAIN: &[u8; 10] = b"PermBranch";

/// Compute the hash of an empty permission leaf
pub fn perm_empty_leaf_hash() -> [u32; 8] {
    let hasher = sha2::Sha256::new_with_prefix(PERM_EMPTY_DOMAIN);
    let result: [u8; 32] = hasher.finalize().into();
    crate::bytes_to_words(result)
}

// ANCHOR: perm_leaf_hash
/// Compute the hash of a permission leaf: sha256("PermLeaf" || spk_bytes || amount_le_bytes)
pub fn perm_leaf_hash(spk: &[u8], amount: u64) -> [u32; 8] {
    let mut hasher = sha2::Sha256::new_with_prefix(PERM_LEAF_DOMAIN);
    hasher.update(spk);
    hasher.update(amount.to_le_bytes());
    let result: [u8; 32] = hasher.finalize().into();
    crate::bytes_to_words(result)
}
// ANCHOR_END: perm_leaf_hash

/// Compute the hash of two sibling nodes: sha256("PermBranch" || left || right)
pub fn perm_branch_hash(left: &[u32; 8], right: &[u32; 8]) -> [u32; 8] {
    let mut hasher = sha2::Sha256::new_with_prefix(PERM_BRANCH_DOMAIN);
    hasher.update(bytemuck::bytes_of(left));
    hasher.update(bytemuck::bytes_of(right));
    let result: [u8; 32] = hasher.finalize().into();
    crate::bytes_to_words(result)
}

/// Permission-tree Merkle hash operations — SHA256 with PermEmpty padding.
pub struct PermHashOps;

impl MerkleHashOps for PermHashOps {
    fn branch(left: &[u32; 8], right: &[u32; 8]) -> [u32; 8] {
        perm_branch_hash(left, right)
    }

    fn empty_subtree(level: usize) -> [u32; 8] {
        perm_empty_subtree_hash(level)
    }
}

/// Streaming permission tree builder — no heap allocation.
/// Uses the generic [`StreamingMerkle`] with permission-tree hash ops.
pub type StreamingPermTreeBuilder = StreamingMerkle<PermHashOps>;

// ANCHOR: required_depth
/// Compute the required depth for a given leaf count.
/// Returns `ceil(log2(count))`, minimum 1, maximum `PERM_MAX_DEPTH`.
pub fn required_depth(count: usize) -> usize {
    if count <= 1 {
        return 1;
    }
    let bits = usize::BITS - (count - 1).leading_zeros();
    (bits as usize).min(PERM_MAX_DEPTH)
}
// ANCHOR_END: required_depth

/// Compute the hash of an empty subtree at a given depth.
pub fn perm_empty_subtree_hash(depth: usize) -> [u32; 8] {
    let mut current = perm_empty_leaf_hash();
    for _ in 0..depth {
        current = perm_branch_hash(&current, &current);
    }
    current
}

// ANCHOR: perm_proof
/// Permission tree proof: variable-depth array of siblings + leaf index.
///
/// Max depth is 8 (256 leaves). Siblings are stored from leaf (level 0)
/// to root (level depth-1).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermProof {
    /// Sibling hashes from leaf to root
    pub siblings: [[u32; 8]; PERM_MAX_DEPTH],
    /// Number of valid siblings (= tree depth)
    pub depth: usize,
    /// Leaf index in the tree
    pub index: usize,
}

impl PermProof {
    /// Create a new proof
    pub fn new(siblings: [[u32; 8]; PERM_MAX_DEPTH], depth: usize, index: usize) -> Self {
        Self { siblings, depth, index }
    }

    /// Compute the root from a leaf hash using this proof
    pub fn compute_root(&self, leaf_hash: &[u32; 8]) -> [u32; 8] {
        let mut current = *leaf_hash;
        for level in 0..self.depth {
            let bit = (self.index >> level) & 1;
            if bit == 0 {
                current = perm_branch_hash(&current, &self.siblings[level]);
            } else {
                current = perm_branch_hash(&self.siblings[level], &current);
            }
        }
        current
    }

    /// Verify that a leaf with given hash exists at this proof's index under given root
    pub fn verify(&self, root: &[u32; 8], leaf_hash: &[u32; 8]) -> bool {
        self.compute_root(leaf_hash) == *root
    }

    /// Compute a new root after replacing the leaf at this proof's index.
    /// Uses the same siblings and index, just a different leaf hash.
    pub fn compute_new_root(&self, new_leaf_hash: &[u32; 8]) -> [u32; 8] {
        self.compute_root(new_leaf_hash)
    }
}
// ANCHOR_END: perm_proof

/// Compute the permission tree root from a slice of (spk, amount) leaves.
///
/// Uses the streaming builder internally — no heap allocation.
/// Returns the root hash and the depth used.
/// Empty trees use depth=1 (2 empty leaves).
pub fn compute_permission_root(leaves: &[(&[u8], u64)]) -> ([u32; 8], usize) {
    if leaves.is_empty() {
        return (perm_empty_subtree_hash(1), 1);
    }
    let depth = required_depth(leaves.len());
    let mut builder = StreamingPermTreeBuilder::new();
    for &(spk, amount) in leaves {
        builder.add_leaf(perm_leaf_hash(spk, amount));
    }
    let count = builder.leaf_count();
    let root = pad_to_depth(builder.finalize(), count, depth);
    (root, depth)
}

// ANCHOR: pad_to_depth
/// Pad a streaming builder result up to a target depth.
///
/// The streaming builder produces a root whose effective depth is
/// `ceil(log2(leaf_count))`. If `target_depth` is larger, we need to
/// extend with empty subtrees on the right.
pub fn pad_to_depth(mut hash: [u32; 8], leaf_count: u32, target_depth: usize) -> [u32; 8] {
    if leaf_count == 0 {
        return perm_empty_subtree_hash(target_depth);
    }
    let effective_depth = required_depth(leaf_count as usize);
    for level in effective_depth..target_depth {
        hash = perm_branch_hash(&hash, &perm_empty_subtree_hash(level));
    }
    hash
}
// ANCHOR_END: pad_to_depth

/// Host-side permission tree: full in-memory tree.
/// Builds from a list of (spk, amount) pairs, generates proofs, computes root.
#[cfg(feature = "std")]
pub struct PermissionTree {
    /// Leaf data: (spk_bytes, amount)
    leaves: Vec<(Vec<u8>, u64)>,
    /// Tree depth
    depth: usize,
    /// All node hashes, indexed by level then position.
    /// Level 0 = leaves, level `depth` = root.
    nodes: Vec<Vec<[u32; 8]>>,
}

#[cfg(feature = "std")]
impl PermissionTree {
    /// Create an empty permission tree
    pub fn new() -> Self {
        let mut tree = Self { leaves: Vec::new(), depth: 1, nodes: Vec::new() };
        tree.rebuild();
        tree
    }

    /// Build a permission tree from a list of (spk, amount) pairs
    pub fn from_leaves(leaves: Vec<(Vec<u8>, u64)>) -> Self {
        let depth = if leaves.is_empty() { 1 } else { required_depth(leaves.len()) };
        let mut tree = Self { leaves, depth, nodes: Vec::new() };
        tree.rebuild();
        tree
    }

    /// Add a leaf and rebuild
    pub fn add_leaf(&mut self, spk: Vec<u8>, amount: u64) {
        self.leaves.push((spk, amount));
        self.depth = required_depth(self.leaves.len());
        self.rebuild();
    }

    /// Get the number of leaves
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Get the tree depth
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Compute the root hash
    pub fn root(&self) -> [u32; 8] {
        self.nodes[self.depth][0]
    }

    /// Generate a proof for the leaf at the given index
    pub fn prove(&self, index: usize) -> PermProof {
        assert!(index < self.leaves.len(), "index out of range");

        let mut siblings = [[0u32; 8]; PERM_MAX_DEPTH];
        let mut current_idx = index;
        for (idx, level) in siblings.iter_mut().enumerate().take(self.depth) {
            let sibling_idx = current_idx ^ 1;
            *level = self.nodes[idx][sibling_idx];
            current_idx /= 2;
        }

        PermProof::new(siblings, self.depth, index)
    }

    /// Update the amount for a leaf at the given index
    pub fn update_amount(&mut self, index: usize, new_amount: u64) {
        assert!(index < self.leaves.len(), "index out of range");
        self.leaves[index].1 = new_amount;
        self.rebuild();
    }

    /// Get leaf data at index
    pub fn get_leaf(&self, index: usize) -> Option<(&[u8], u64)> {
        self.leaves.get(index).map(|(spk, amount): &(Vec<u8>, u64)| (spk.as_slice(), *amount))
    }

    fn rebuild(&mut self) {
        let capacity = 1 << self.depth;
        let empty = perm_empty_leaf_hash();

        // Build leaf level
        let mut leaf_level = vec![empty; capacity];
        for (i, (spk, amount)) in self.leaves.iter().enumerate() {
            if !spk.is_empty() {
                leaf_level[i] = perm_leaf_hash(spk, *amount);
            }
            // else: withdrawn leaf — stays as perm_empty_leaf_hash()
        }

        let mut nodes = vec![leaf_level];

        // Build tree bottom-up
        for _level in 0..self.depth {
            let prev = nodes.last().unwrap();
            let next_size = prev.len() / 2;
            let mut next_level = Vec::with_capacity(next_size);
            for i in 0..next_size {
                next_level.push(perm_branch_hash(&prev[2 * i], &prev[2 * i + 1]));
            }
            nodes.push(next_level);
        }

        self.nodes = nodes;
    }
}

#[cfg(feature = "std")]
impl Default for PermissionTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn test_empty_leaf_deterministic() {
        let h1 = perm_empty_leaf_hash();
        let h2 = perm_empty_leaf_hash();
        assert_eq!(h1, h2);
        // Must differ from SMT's empty leaf
        assert_ne!(h1, crate::smt::empty_leaf_hash());
    }

    #[test]
    fn test_leaf_hash_different_from_smt() {
        let spk = [0x42u8; 34];
        let perm_h = perm_leaf_hash(&spk, 1000);
        let smt_h = crate::smt::leaf_hash(&[0x42424242u32; 8], 1000);
        assert_ne!(perm_h, smt_h);
    }

    #[test]
    fn test_required_depth() {
        assert_eq!(required_depth(0), 1);
        assert_eq!(required_depth(1), 1);
        assert_eq!(required_depth(2), 1);
        assert_eq!(required_depth(3), 2);
        assert_eq!(required_depth(4), 2);
        assert_eq!(required_depth(5), 3);
        assert_eq!(required_depth(8), 3);
        assert_eq!(required_depth(9), 4);
        assert_eq!(required_depth(256), 8);
    }

    #[test]
    fn test_compute_root_single_leaf() {
        let spk = b"test_spk_34_bytes_padding_here!!00";
        let amount = 500u64;
        let (root, depth) = compute_permission_root(&[(spk, amount)]);
        assert_eq!(depth, 1);

        // Manually: leaf_hash || empty → branch
        let lh = perm_leaf_hash(spk, amount);
        let eh = perm_empty_leaf_hash();
        let expected = perm_branch_hash(&lh, &eh);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_compute_root_two_leaves() {
        let spk1 = b"spk1_34bytes_aaaaaaaaaaaaaaaaaaa";
        let spk2 = b"spk2_34bytes_bbbbbbbbbbbbbbbbbbb";
        let (root, depth) = compute_permission_root(&[(spk1, 100), (spk2, 200)]);
        assert_eq!(depth, 1);

        let lh1 = perm_leaf_hash(spk1, 100);
        let lh2 = perm_leaf_hash(spk2, 200);
        let expected = perm_branch_hash(&lh1, &lh2);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_compute_root_three_leaves() {
        let spk1 = b"spk1_34bytes_aaaaaaaaaaaaaaaaaaa";
        let spk2 = b"spk2_34bytes_bbbbbbbbbbbbbbbbbbb";
        let spk3 = b"spk3_34bytes_ccccccccccccccccccc";
        let (root, depth) = compute_permission_root(&[(&spk1[..], 100), (&spk2[..], 200), (&spk3[..], 300)]);
        assert_eq!(depth, 2);

        let lh1 = perm_leaf_hash(spk1, 100);
        let lh2 = perm_leaf_hash(spk2, 200);
        let lh3 = perm_leaf_hash(spk3, 300);
        let eh = perm_empty_leaf_hash();
        let b01 = perm_branch_hash(&lh1, &lh2);
        let b23 = perm_branch_hash(&lh3, &eh);
        let expected = perm_branch_hash(&b01, &b23);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_compute_root_four_leaves() {
        let leaves: Vec<(Vec<u8>, u64)> = (0..4u8).map(|i| (vec![i; 34], (i as u64 + 1) * 100)).collect();
        let refs: Vec<(&[u8], u64)> = leaves.iter().map(|(s, a)| (s.as_slice(), *a)).collect();
        let (root, depth) = compute_permission_root(&refs);
        assert_eq!(depth, 2);

        let lh: Vec<_> = leaves.iter().map(|(s, a)| perm_leaf_hash(s, *a)).collect();
        let b01 = perm_branch_hash(&lh[0], &lh[1]);
        let b23 = perm_branch_hash(&lh[2], &lh[3]);
        let expected = perm_branch_hash(&b01, &b23);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_compute_root_eight_leaves() {
        let leaves: Vec<(Vec<u8>, u64)> = (0..8u8).map(|i| (vec![i; 35], (i as u64 + 1) * 50)).collect();
        let refs: Vec<(&[u8], u64)> = leaves.iter().map(|(s, a)| (s.as_slice(), *a)).collect();
        let (root, depth) = compute_permission_root(&refs);
        assert_eq!(depth, 3);
        // Just verify it's deterministic
        let (root2, _) = compute_permission_root(&refs);
        assert_eq!(root, root2);
    }

    #[test]
    fn test_empty_tree_root() {
        let (root, depth) = compute_permission_root(&[]);
        assert_eq!(depth, 1);
        let expected = perm_empty_subtree_hash(1);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_various_spk_sizes() {
        // P2PK = 34 bytes
        let spk_34 = [0xAAu8; 34];
        // P2SH = 35 bytes
        let spk_35 = [0xBBu8; 35];
        // Padded = 36 bytes
        let spk_36 = [0xCCu8; 36];

        let h34 = perm_leaf_hash(&spk_34, 100);
        let h35 = perm_leaf_hash(&spk_35, 100);
        let h36 = perm_leaf_hash(&spk_36, 100);

        // All should be different
        assert_ne!(h34, h35);
        assert_ne!(h35, h36);
        assert_ne!(h34, h36);
    }
}

#[cfg(all(test, feature = "std"))]
mod std_tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn test_permission_tree_empty() {
        let tree = PermissionTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        let root = tree.root();
        let (expected, _) = compute_permission_root(&[]);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_permission_tree_single_leaf() {
        let spk = vec![0x42u8; 34];
        let amount = 1000u64;
        let tree = PermissionTree::from_leaves(vec![(spk.clone(), amount)]);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.depth(), 1);

        let (expected_root, _) = compute_permission_root(&[(&spk, amount)]);
        assert_eq!(tree.root(), expected_root);
    }

    #[test]
    fn test_permission_tree_proof_verify() {
        let leaves: Vec<(Vec<u8>, u64)> = (0..4u8).map(|i| (vec![i; 34], (i as u64 + 1) * 100)).collect();
        let tree = PermissionTree::from_leaves(leaves.clone());
        let root = tree.root();

        for (i, (spk, amount)) in leaves.iter().enumerate() {
            let proof = tree.prove(i);
            let lh = perm_leaf_hash(spk, *amount);
            assert!(proof.verify(&root, &lh), "proof verify failed for index {}", i);
        }
    }

    #[test]
    fn test_permission_tree_proof_verify_wrong_amount() {
        let leaves = vec![(vec![0u8; 34], 100), (vec![1u8; 34], 200)];
        let tree = PermissionTree::from_leaves(leaves);
        let root = tree.root();

        let proof = tree.prove(0);
        let wrong_lh = perm_leaf_hash(&[0u8; 34], 999);
        assert!(!proof.verify(&root, &wrong_lh));
    }

    #[test]
    fn test_permission_tree_root_update() {
        let leaves = vec![(vec![0xAAu8; 34], 500), (vec![0xBBu8; 35], 300)];
        let tree = PermissionTree::from_leaves(leaves.clone());
        let old_root = tree.root();

        // Get proof for leaf 0
        let proof = tree.prove(0);
        let old_lh = perm_leaf_hash(&[0xAAu8; 34], 500);
        assert!(proof.verify(&old_root, &old_lh));

        // Compute new root via proof with updated amount
        let new_lh = perm_leaf_hash(&[0xAAu8; 34], 200);
        let new_root_via_proof = proof.compute_new_root(&new_lh);

        // Build new tree with updated amount and compare
        let updated_tree = PermissionTree::from_leaves(vec![(vec![0xAAu8; 34], 200), (vec![0xBBu8; 35], 300)]);
        assert_eq!(new_root_via_proof, updated_tree.root());
        assert_ne!(old_root, new_root_via_proof);

        // Also test the update_amount method
        let mut tree2 = PermissionTree::from_leaves(vec![(vec![0xAAu8; 34], 500), (vec![0xBBu8; 35], 300)]);
        tree2.update_amount(0, 200);
        assert_eq!(tree2.root(), updated_tree.root());
    }

    #[test]
    fn test_permission_tree_root_update_to_empty() {
        let leaves = vec![(vec![0xAAu8; 34], 500), (vec![0xBBu8; 35], 300)];
        let tree = PermissionTree::from_leaves(leaves);
        let root = tree.root();

        // Get proof for leaf 0
        let proof = tree.prove(0);

        // Compute new root with empty leaf (amount == 0 → claim fully withdrawn)
        let empty_lh = perm_empty_leaf_hash();
        let new_root = proof.compute_new_root(&empty_lh);
        assert_ne!(root, new_root);

        // Verify: new root should match a tree where leaf 0 is empty
        let lh1 = perm_empty_leaf_hash();
        let lh2 = perm_leaf_hash(&[0xBBu8; 35], 300);
        let expected = perm_branch_hash(&lh1, &lh2);
        assert_eq!(new_root, expected);
    }

    #[test]
    fn test_permission_tree_add_leaf() {
        let mut tree = PermissionTree::new();
        tree.add_leaf(vec![0xAAu8; 34], 100);
        tree.add_leaf(vec![0xBBu8; 35], 200);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.depth(), 1);

        let expected_tree = PermissionTree::from_leaves(vec![(vec![0xAAu8; 34], 100), (vec![0xBBu8; 35], 200)]);
        assert_eq!(tree.root(), expected_tree.root());
    }

    #[test]
    fn test_permission_tree_get_leaf() {
        let tree = PermissionTree::from_leaves(vec![(vec![0xAAu8; 34], 100), (vec![0xBBu8; 35], 200)]);
        let (spk, amount) = tree.get_leaf(0).unwrap();
        assert_eq!(spk, &[0xAAu8; 34]);
        assert_eq!(amount, 100);
        let (spk, amount) = tree.get_leaf(1).unwrap();
        assert_eq!(spk, &[0xBBu8; 35]);
        assert_eq!(amount, 200);
        assert!(tree.get_leaf(2).is_none());
    }

    #[test]
    fn test_permission_tree_256_leaves() {
        let leaves: Vec<(Vec<u8>, u64)> = (0..256u16).map(|i| (vec![i as u8; 34], i as u64 * 10)).collect();
        let tree = PermissionTree::from_leaves(leaves.clone());
        assert_eq!(tree.depth(), 8);

        let root = tree.root();
        // Verify a few proofs
        for i in [0, 1, 127, 255] {
            let proof = tree.prove(i);
            let lh = perm_leaf_hash(&leaves[i].0, leaves[i].1);
            assert!(proof.verify(&root, &lh), "proof verify failed for index {}", i);
        }
    }

    #[test]
    fn test_compute_root_matches_tree() {
        let leaves_data: Vec<(Vec<u8>, u64)> = (0..7u8).map(|i| (vec![i; 34], (i as u64 + 1) * 100)).collect();
        let refs: Vec<(&[u8], u64)> = leaves_data.iter().map(|(s, a)| (s.as_slice(), *a)).collect();
        let (root_fn, depth_fn) = compute_permission_root(&refs);
        let tree = PermissionTree::from_leaves(leaves_data);
        assert_eq!(tree.root(), root_fn);
        assert_eq!(tree.depth(), depth_fn);
    }
}
