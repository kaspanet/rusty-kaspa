//! Sparse Merkle Tree proof types and `no_std` verification.
//!
//! A proof consists of 256 sibling hashes compressed via a 32-byte bitmap.
//! When a sibling equals the canonical empty subtree hash for its level,
//! the bitmap bit is set and the sibling is omitted. This typically reduces
//! proof size from 8 KiB to a few hundred bytes.
//!
//! - [`SmtProof`] — borrowed view, zero-alloc, usable in `no_std` / ZK.
//! - [`OwnedSmtProof`] — owned, delegates to `SmtProof` via `as_proof()`.

use alloc::vec::Vec;
use kaspa_hashes::{Hash, ZERO_HASH};

use crate::store::{BranchChildren, BranchKey};
use crate::tree::SmtBranchChanges;
use crate::{DEPTH, SmtHasher, bit_at, hash_node};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SmtProofError {
    #[error("sibling count mismatch: bitmap implies {expected} non-empty siblings, but got {actual}")]
    SiblingCountMismatch { expected: usize, actual: usize },
}

#[inline]
fn is_empty_at_depth(bitmap: &[u8; 32], d: usize) -> bool {
    bitmap[d / 8] & (1 << (d % 8)) != 0
}

fn bitmap_clear_count(bitmap: &[u8; 32]) -> usize {
    DEPTH - bitmap.iter().map(|b| b.count_ones() as usize).sum::<usize>()
}

/// Compute the Merkle root from a proof.
/// If `cache` is provided, each branch is looked up first (skip hash_node if found)
/// and newly computed branches are inserted.
fn compute_root_inner<H: SmtHasher>(
    bitmap: &[u8; 32],
    siblings: &[Hash],
    key: &Hash,
    leaf_hash: Option<Hash>,
    mut cache: Option<&mut SmtBranchChanges>,
) -> Result<Hash, SmtProofError> {
    let expected = bitmap_clear_count(bitmap);
    if siblings.len() != expected {
        return Err(SmtProofError::SiblingCountMismatch { expected, actual: siblings.len() });
    }

    let mut current = leaf_hash.unwrap_or(ZERO_HASH);
    let mut sib_idx = siblings.len();

    for d in (0..DEPTH).rev() {
        let height = (DEPTH - 1 - d) as u8;
        let sibling = if is_empty_at_depth(bitmap, d) {
            H::EMPTY_HASHES[height as usize]
        } else {
            sib_idx -= 1;
            siblings[sib_idx]
        };

        let (left, right) = if bit_at(key, d) { (sibling, current) } else { (current, sibling) };

        current = if let Some(ref mut cache) = cache {
            let bk = BranchKey::new(height, key);
            if let Some(cached) = cache.get(&bk) {
                hash_node::<H>(cached.left, cached.right)
            } else {
                cache.insert(bk, BranchChildren { left, right });
                hash_node::<H>(left, right)
            }
        } else {
            hash_node::<H>(left, right)
        };
    }

    Ok(current)
}

/// Borrowed compressed proof for a 256-bit Sparse Merkle Tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmtProof<'a> {
    pub bitmap: &'a [u8; 32],
    pub siblings: &'a [Hash],
}

impl<'a> SmtProof<'a> {
    pub fn compute_root<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>) -> Result<Hash, SmtProofError> {
        compute_root_inner::<H>(self.bitmap, self.siblings, key, leaf_hash, None)
    }

    pub fn verify<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>, root: Hash) -> Result<bool, SmtProofError> {
        Ok(self.compute_root::<H>(key, leaf_hash)? == root)
    }

    /// Verify the proof while populating `cache` with intermediate branches.
    /// Branches already in the cache are reused (hash_node skipped).
    pub fn verify_cached<H: SmtHasher>(
        &self,
        key: &Hash,
        leaf_hash: Option<Hash>,
        root: Hash,
        cache: &mut SmtBranchChanges,
    ) -> Result<bool, SmtProofError> {
        let computed = compute_root_inner::<H>(self.bitmap, self.siblings, key, leaf_hash, Some(cache))?;
        Ok(computed == root)
    }

    pub fn non_empty_count(&self) -> usize {
        self.siblings.len()
    }

    pub fn empty_count(&self) -> usize {
        DEPTH - self.siblings.len()
    }
}

/// Owned compressed proof for a 256-bit Sparse Merkle Tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedSmtProof {
    pub bitmap: [u8; 32],
    pub siblings: Vec<Hash>,
}

impl OwnedSmtProof {
    pub fn as_proof(&self) -> SmtProof<'_> {
        SmtProof { bitmap: &self.bitmap, siblings: &self.siblings }
    }

    pub fn compute_root<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>) -> Result<Hash, SmtProofError> {
        self.as_proof().compute_root::<H>(key, leaf_hash)
    }

    pub fn verify<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>, root: Hash) -> Result<bool, SmtProofError> {
        self.as_proof().verify::<H>(key, leaf_hash, root)
    }

    pub fn non_empty_count(&self) -> usize {
        self.siblings.len()
    }

    pub fn empty_count(&self) -> usize {
        DEPTH - self.siblings.len()
    }
}
