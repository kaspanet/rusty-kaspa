//! Sparse Merkle Tree implementation for account-based rollup.
//!
//! This module provides an 8-level SMT (256 accounts max) for the demo.
//! Keys are mapped using the first byte of the pubkey.

use sha2::Digest;

/// Tree depth: 8 levels = 256 possible leaves
pub const SMT_DEPTH: usize = 8;

/// Domain prefix for leaf hashing
const LEAF_DOMAIN: &[u8; 7] = b"SMTLeaf";

/// Domain prefix for empty leaf
const EMPTY_DOMAIN: &[u8; 8] = b"SMTEmpty";

/// Domain prefix for branch hashing
const BRANCH_DOMAIN: &[u8; 9] = b"SMTBranch";

/// Compute the hash of an empty leaf
pub fn empty_leaf_hash() -> [u32; 8] {
    let hasher = sha2::Sha256::new_with_prefix(EMPTY_DOMAIN);
    let result: [u8; 32] = hasher.finalize().into();
    crate::bytes_to_words(result)
}

/// Compute the hash of an account leaf: sha256("SMTLeaf" || pubkey || balance_le_bytes)
pub fn leaf_hash(pubkey: &[u32; 8], balance: u64) -> [u32; 8] {
    let mut hasher = sha2::Sha256::new_with_prefix(LEAF_DOMAIN);
    hasher.update(bytemuck::bytes_of(pubkey));
    hasher.update(balance.to_le_bytes());
    let result: [u8; 32] = hasher.finalize().into();
    crate::bytes_to_words(result)
}

/// Compute the hash of two sibling nodes: sha256("SMTBranch" || left || right)
pub fn branch_hash(left: &[u32; 8], right: &[u32; 8]) -> [u32; 8] {
    let mut hasher = sha2::Sha256::new_with_prefix(BRANCH_DOMAIN);
    hasher.update(bytemuck::bytes_of(left));
    hasher.update(bytemuck::bytes_of(right));
    let result: [u8; 32] = hasher.finalize().into();
    crate::bytes_to_words(result)
}

/// Get the key index (0-255) from a pubkey (uses first byte)
pub fn key_to_index(pubkey: &[u32; 8]) -> u8 {
    bytemuck::bytes_of(pubkey)[0]
}

/// SMT proof structure for 8-level tree
/// Contains sibling hashes at each level from leaf to root
#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct SmtProof {
    /// Sibling hashes from leaf (level 0) to root (level 7)
    pub siblings: [[u32; 8]; SMT_DEPTH],
}

impl SmtProof {
    /// Create an empty proof (all zeros - used for new trees)
    pub fn empty() -> Self {
        // For an empty tree, all siblings are empty subtree hashes at each level
        let mut siblings = [[0u32; 8]; SMT_DEPTH];
        let mut current = empty_leaf_hash();
        for sibling in &mut siblings {
            *sibling = current;
            current = branch_hash(&current, &current);
        }
        Self { siblings }
    }

    /// Compute the root given a leaf hash and key
    pub fn compute_root(&self, leaf_hash: &[u32; 8], key: u8) -> [u32; 8] {
        let mut current = *leaf_hash;
        for (level, sibling) in self.siblings.iter().enumerate() {
            let bit = (key >> level) & 1;
            if bit == 0 {
                current = branch_hash(&current, sibling);
            } else {
                current = branch_hash(sibling, &current);
            }
        }
        current
    }

    /// Verify that a leaf with given hash exists at key under given root
    pub fn verify(&self, root: &[u32; 8], key: u8, leaf_hash: &[u32; 8]) -> bool {
        self.compute_root(leaf_hash, key) == *root
    }
}

impl Default for SmtProof {
    fn default() -> Self {
        Self::empty()
    }
}

/// In-memory Sparse Merkle Tree for the host (prover side)
/// Not used in guest - guest only verifies proofs
#[cfg(feature = "std")]
pub struct Smt {
    /// Leaf values: (pubkey, balance) or None for empty
    leaves: [Option<([u32; 8], u64)>; 256],
    /// Cached node hashes for each level
    /// nodes[level][index] where level 0 = leaves, level 8 = root
    nodes: [[Option<[u32; 8]>; 256]; SMT_DEPTH + 1],
}

#[cfg(feature = "std")]
impl Default for Smt {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "std")]
impl Smt {
    /// Create a new empty SMT
    pub fn new() -> Self {
        Self { leaves: [None; 256], nodes: [[None; 256]; SMT_DEPTH + 1] }
    }

    /// Get hash of an empty subtree at given level
    fn empty_subtree_hash(&self, level: usize) -> [u32; 8] {
        let mut current = empty_leaf_hash();
        for _ in 0..level {
            current = branch_hash(&current, &current);
        }
        current
    }

    /// Insert or update an account
    pub fn insert(&mut self, pubkey: [u32; 8], balance: u64) {
        let key = key_to_index(&pubkey) as usize;
        self.leaves[key] = Some((pubkey, balance));
        // Invalidate cached nodes on the path to root
        self.invalidate_path(key);
    }

    /// Get account at key
    pub fn get(&self, pubkey: &[u32; 8]) -> Option<u64> {
        let key = key_to_index(pubkey) as usize;
        self.leaves[key].as_ref().and_then(|(pk, bal)| if pk == pubkey { Some(*bal) } else { None })
    }

    /// Get account by key index
    pub fn get_by_index(&self, key: u8) -> Option<([u32; 8], u64)> {
        self.leaves[key as usize]
    }

    /// Invalidate cached nodes on path from leaf to root
    fn invalidate_path(&mut self, key: usize) {
        let mut idx = key;
        for level in 0..=SMT_DEPTH {
            if level < SMT_DEPTH {
                let node_count = 1 << (SMT_DEPTH - level);
                if idx < node_count {
                    self.nodes[level][idx] = None;
                }
            }
            idx /= 2;
        }
    }

    /// Compute the root hash
    pub fn root(&self) -> [u32; 8] {
        self.compute_root_recursive(SMT_DEPTH, 0)
    }

    /// Recursively compute node hash
    fn compute_root_recursive(&self, level: usize, index: usize) -> [u32; 8] {
        if level == 0 {
            // Leaf level
            match &self.leaves[index] {
                Some((pubkey, balance)) => leaf_hash(pubkey, *balance),
                None => empty_leaf_hash(),
            }
        } else {
            let left_idx = index * 2;
            let right_idx = index * 2 + 1;
            let child_level = level - 1;
            let max_at_child_level = 1 << (SMT_DEPTH - child_level);

            let left = if left_idx < max_at_child_level {
                self.compute_root_recursive(child_level, left_idx)
            } else {
                self.empty_subtree_hash(child_level)
            };

            let right = if right_idx < max_at_child_level {
                self.compute_root_recursive(child_level, right_idx)
            } else {
                self.empty_subtree_hash(child_level)
            };

            branch_hash(&left, &right)
        }
    }

    /// Generate a proof for the given key
    pub fn prove(&self, pubkey: &[u32; 8]) -> SmtProof {
        let key = key_to_index(pubkey) as usize;
        let mut siblings = [[0u32; 8]; SMT_DEPTH];

        let mut current_idx = key;
        for level in 0..SMT_DEPTH {
            // Sibling index
            let sibling_idx = current_idx ^ 1;
            siblings[level] = self.compute_root_recursive(level, sibling_idx);
            current_idx /= 2;
        }

        SmtProof { siblings }
    }

    /// Update an existing account's balance (returns old balance if exists)
    pub fn update(&mut self, pubkey: &[u32; 8], new_balance: u64) -> Option<u64> {
        let key = key_to_index(pubkey) as usize;
        let old = self.leaves[key].as_ref().and_then(|(pk, bal)| if pk == pubkey { Some(*bal) } else { None });
        if old.is_some() {
            self.leaves[key] = Some((*pubkey, new_balance));
            self.invalidate_path(key);
        }
        old
    }

    /// Insert or update - returns true if it was an update
    pub fn upsert(&mut self, pubkey: [u32; 8], balance: u64) -> bool {
        let key = key_to_index(&pubkey) as usize;
        let was_update = self.leaves[key].is_some();
        self.leaves[key] = Some((pubkey, balance));
        self.invalidate_path(key);
        was_update
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    fn make_pubkey(seed: u8) -> [u32; 8] {
        // First byte is the key index, construct directly as u32
        // seed goes in low byte, seed*7 in next byte
        let first_word = (seed as u32) | ((seed.wrapping_mul(7) as u32) << 8);
        // seed+1 goes in high byte of last word
        let last_word = (seed.wrapping_add(1) as u32) << 24;
        [first_word, 0, 0, 0, 0, 0, 0, last_word]
    }

    #[test]
    fn test_empty_tree() {
        let smt = Smt::new();
        let root1 = smt.root();

        // Empty tree should have deterministic root
        let smt2 = Smt::new();
        let root2 = smt2.root();
        assert_eq!(root1, root2);
    }

    #[test]
    fn test_single_insert() {
        let mut smt = Smt::new();
        let empty_root = smt.root();

        let pk = make_pubkey(42);
        smt.insert(pk, 1000);

        let new_root = smt.root();
        assert_ne!(empty_root, new_root);

        // Verify we can get the value back
        assert_eq!(smt.get(&pk), Some(1000));
    }

    #[test]
    fn test_proof_verification() {
        let mut smt = Smt::new();
        let pk = make_pubkey(42);
        smt.insert(pk, 1000);

        let root = smt.root();
        let proof = smt.prove(&pk);

        // Verify proof
        let lh = leaf_hash(&pk, 1000);
        let key = key_to_index(&pk);
        assert!(proof.verify(&root, key, &lh));

        // Wrong balance should fail
        let wrong_lh = leaf_hash(&pk, 999);
        assert!(!proof.verify(&root, key, &wrong_lh));
    }

    #[test]
    fn test_multiple_accounts() {
        let mut smt = Smt::new();

        let alice = make_pubkey(10);
        let bob = make_pubkey(20);
        let charlie = make_pubkey(30);

        smt.insert(alice, 1000);
        smt.insert(bob, 500);
        smt.insert(charlie, 0);

        let root = smt.root();

        // Verify all proofs
        for (pk, balance) in [(alice, 1000), (bob, 500), (charlie, 0)] {
            let proof = smt.prove(&pk);
            let lh = leaf_hash(&pk, balance);
            let key = key_to_index(&pk);
            assert!(proof.verify(&root, key, &lh), "Proof failed for {:?}", bytemuck::bytes_of(&pk)[0]);
        }
    }

    #[test]
    fn test_update() {
        let mut smt = Smt::new();
        let pk = make_pubkey(42);

        smt.insert(pk, 1000);
        let root1 = smt.root();

        smt.upsert(pk, 500);
        let root2 = smt.root();

        assert_ne!(root1, root2);
        assert_eq!(smt.get(&pk), Some(500));
    }

    #[test]
    fn test_proof_after_update() {
        let mut smt = Smt::new();
        let alice = make_pubkey(10);
        let bob = make_pubkey(20);

        smt.insert(alice, 1000);
        smt.insert(bob, 500);

        // Update alice's balance
        smt.upsert(alice, 900);
        smt.upsert(bob, 600);

        let root = smt.root();

        // Verify both proofs with new balances
        let alice_proof = smt.prove(&alice);
        let bob_proof = smt.prove(&bob);

        assert!(alice_proof.verify(&root, key_to_index(&alice), &leaf_hash(&alice, 900)));
        assert!(bob_proof.verify(&root, key_to_index(&bob), &leaf_hash(&bob, 600)));
    }

    #[test]
    fn test_empty_proof_for_new_account() {
        let mut smt = Smt::new();
        let alice = make_pubkey(10);
        smt.insert(alice, 1000);

        // Get proof for non-existent account
        let bob = make_pubkey(20);
        let root = smt.root();
        let proof = smt.prove(&bob);

        // Should verify as empty
        let empty = empty_leaf_hash();
        assert!(proof.verify(&root, key_to_index(&bob), &empty));
    }
}
