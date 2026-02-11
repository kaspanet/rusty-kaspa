//! Account-based state for the rollup.
//!
//! The state is represented by an SMT root hash of all accounts.
//! Accounts are (pubkey, balance) pairs.
//!
//! All types use `[u32; 8]` for 32-byte hashes for zkVM efficiency.

use crate::smt::{self, SMT_DEPTH, SmtProof};

/// Account structure (40 bytes)
#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Account {
    /// Account pubkey (32 bytes as [u32; 8])
    pub pubkey: [u32; 8],
    /// Account balance (8 bytes)
    pub balance: u64,
}

impl Account {
    /// Create a new account
    pub fn new(pubkey: [u32; 8], balance: u64) -> Self {
        Self { pubkey, balance }
    }

    /// Compute the leaf hash for this account
    pub fn leaf_hash(&self) -> [u32; 8] {
        smt::leaf_hash(&self.pubkey, self.balance)
    }
}

/// Witness for a single account in the SMT
#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct AccountWitness {
    /// Account pubkey (32 bytes as [u32; 8])
    pub pubkey: [u32; 8],
    /// Account balance (8 bytes)
    pub balance: u64,
    /// SMT proof for this account
    pub proof: SmtProof,
}

impl AccountWitness {
    /// Size in bytes
    pub const SIZE: usize = core::mem::size_of::<Self>();

    /// Size in u32 words
    pub const WORDS: usize = Self::SIZE / 4;

    /// Create a new account witness
    pub fn new(pubkey: [u32; 8], balance: u64, proof: SmtProof) -> Self {
        Self { pubkey, balance, proof }
    }

    /// Compute the leaf hash
    pub fn leaf_hash(&self) -> [u32; 8] {
        smt::leaf_hash(&self.pubkey, self.balance)
    }

    /// Verify this witness against a root
    pub fn verify(&self, root: &[u32; 8]) -> bool {
        let key = smt::key_to_index(&self.pubkey);
        self.proof.verify(root, key, &self.leaf_hash())
    }

    /// Verify this is an empty slot
    pub fn verify_empty(&self, root: &[u32; 8]) -> bool {
        let key = smt::key_to_index(&self.pubkey);
        let empty = smt::empty_leaf_hash();
        self.proof.verify(root, key, &empty)
    }

    /// Compute new root after updating balance
    pub fn compute_new_root(&self, new_balance: u64) -> [u32; 8] {
        let key = smt::key_to_index(&self.pubkey);
        let new_leaf = smt::leaf_hash(&self.pubkey, new_balance);
        self.proof.compute_root(&new_leaf, key)
    }

    /// Convert to word slice
    pub fn as_words(&self) -> &[u32] {
        bytemuck::cast_slice(bytemuck::bytes_of(self))
    }

    /// Convert to bytes
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}

/// State root type (32 bytes as [u32; 8])
pub type StateRoot = [u32; 8];

/// Compute empty tree root
pub fn empty_tree_root() -> StateRoot {
    let empty_leaf = smt::empty_leaf_hash();
    let mut current = empty_leaf;
    for _ in 0..SMT_DEPTH {
        current = smt::branch_hash(&current, &current);
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_witness_size() {
        // 32 + 8 + (32 * 8) = 32 + 8 + 256 = 296 bytes
        assert_eq!(AccountWitness::SIZE, 296);
    }

    #[test]
    fn test_account_leaf_hash() {
        let pk: [u32; 8] = [0x42424242; 8];
        let account = Account::new(pk, 1000);
        let hash1 = account.leaf_hash();
        let hash2 = smt::leaf_hash(&pk, 1000);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_empty_tree_root_is_deterministic() {
        let root1 = empty_tree_root();
        let root2 = empty_tree_root();
        assert_eq!(root1, root2);
    }
}
