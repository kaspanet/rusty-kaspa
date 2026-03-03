//! Sparse Merkle Tree proof types and `no_std` verification.
//!
//! # Proof format
//!
//! A proof consists of 256 sibling hashes (one per tree level) compressed
//! via a 32-byte bitmap. When a sibling is the canonical empty subtree hash
//! for its level, the corresponding bitmap bit is set and the sibling is
//! omitted from storage. This typically reduces proof size from 8 KiB to
//! a few hundred bytes for sparse trees.
//!
//! # Types
//!
//! - [`SmtProof`] — borrowed view (`&[u8; 32]` bitmap + `&[Hash]` siblings).
//!   Zero-alloc verification, usable in ZK guest programs without `alloc`.
//! - [`OwnedSmtProof`] — owned (`[u8; 32]` + `Vec<Hash>`). Requires `alloc`.
//!   Delegates to [`SmtProof`] via [`as_proof()`](OwnedSmtProof::as_proof).

use alloc::vec::Vec;
use kaspa_hashes::{Hash, ZERO_HASH};

use crate::{DEPTH, SmtHasher, bit_at, hash_node};

/// Errors that can occur when verifying or computing a root from an [`SmtProof`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SmtProofError {
    /// The number of non-empty siblings does not match the bitmap.
    #[error("sibling count mismatch: bitmap implies {expected} non-empty siblings, but got {actual}")]
    SiblingCountMismatch { expected: usize, actual: usize },
}

/// Check whether the sibling at the given depth is an empty subtree hash.
#[inline]
fn is_empty_at_depth(bitmap: &[u8; 32], d: usize) -> bool {
    bitmap[d / 8] & (1 << (d % 8)) != 0
}

/// Number of clear bits in the bitmap (expected non-empty sibling count).
fn bitmap_clear_count(bitmap: &[u8; 32]) -> usize {
    DEPTH - bitmap.iter().map(|b| b.count_ones() as usize).sum::<usize>()
}

/// Compute the Merkle root from a bitmap, siblings slice, key, and leaf hash.
fn compute_root_inner<H: SmtHasher>(
    bitmap: &[u8; 32],
    siblings: &[Hash],
    key: &Hash,
    leaf_hash: Option<Hash>,
) -> Result<Hash, SmtProofError> {
    let expected = bitmap_clear_count(bitmap);
    if siblings.len() != expected {
        return Err(SmtProofError::SiblingCountMismatch { expected, actual: siblings.len() });
    }

    let mut current = leaf_hash.unwrap_or(ZERO_HASH);

    // Siblings are stored shallowest-first (depth 0 first); we consume
    // them deepest-first (depth 255 first) during the leaf-to-root walk.
    let mut sib_idx = siblings.len();

    for d in (0..DEPTH).rev() {
        // level = height from leaf: depth 255 → level 0, depth 0 → level 255
        let level = DEPTH - 1 - d;
        let sibling = if is_empty_at_depth(bitmap, d) {
            H::EMPTY_HASHES[level]
        } else {
            sib_idx -= 1;
            siblings[sib_idx]
        };

        current = if bit_at(key, d) { hash_node::<H>(sibling, current) } else { hash_node::<H>(current, sibling) };
    }

    Ok(current)
}

/// A borrowed compressed proof for a 256-bit Sparse Merkle Tree.
///
/// Zero-alloc: holds `&[u8; 32]` bitmap and `&[Hash]` siblings.
/// Use this for verification in `no_std` / ZK contexts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmtProof<'a> {
    pub bitmap: &'a [u8; 32],
    pub siblings: &'a [Hash],
}

impl<'a> SmtProof<'a> {
    /// Compute the Merkle root implied by this proof and the given leaf.
    pub fn compute_root<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>) -> Result<Hash, SmtProofError> {
        compute_root_inner::<H>(self.bitmap, self.siblings, key, leaf_hash)
    }

    /// Verify that `key` maps to `leaf_hash` (or is absent) under the given `root`.
    pub fn verify<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>, root: Hash) -> Result<bool, SmtProofError> {
        Ok(self.compute_root::<H>(key, leaf_hash)? == root)
    }

    /// Number of non-empty (explicitly stored) siblings.
    pub fn non_empty_count(&self) -> usize {
        self.siblings.len()
    }

    /// Total number of empty (bitmap-compressed) siblings.
    pub fn empty_count(&self) -> usize {
        DEPTH - self.siblings.len()
    }
}

/// An owned compressed proof for a 256-bit Sparse Merkle Tree.
///
/// Owns its bitmap and siblings `Vec`. Created by [`SparseMerkleTree::prove`](crate::tree::SparseMerkleTree::prove).
/// Use [`as_proof()`](Self::as_proof) to get a borrowed [`SmtProof`] view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedSmtProof {
    pub bitmap: [u8; 32],
    pub siblings: Vec<Hash>,
}

impl OwnedSmtProof {
    /// Borrow as a [`SmtProof`] view.
    pub fn as_proof(&self) -> SmtProof<'_> {
        SmtProof { bitmap: &self.bitmap, siblings: &self.siblings }
    }

    /// Compute the Merkle root implied by this proof and the given leaf.
    pub fn compute_root<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>) -> Result<Hash, SmtProofError> {
        compute_root_inner::<H>(&self.bitmap, &self.siblings, key, leaf_hash)
    }

    /// Verify that `key` maps to `leaf_hash` (or is absent) under the given `root`.
    pub fn verify<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>, root: Hash) -> Result<bool, SmtProofError> {
        Ok(self.compute_root::<H>(key, leaf_hash)? == root)
    }

    /// Number of non-empty (explicitly stored) siblings.
    pub fn non_empty_count(&self) -> usize {
        self.siblings.len()
    }

    /// Total number of empty (bitmap-compressed) siblings.
    pub fn empty_count(&self) -> usize {
        DEPTH - self.siblings.len()
    }
}
