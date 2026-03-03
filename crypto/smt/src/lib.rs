//! # kaspa-smt — 256-bit Sparse Merkle Tree
//!
//! A general-purpose 256-bit Sparse Merkle Tree (SMT) with compressed proofs.
//!
//! ## Feature flags
//!
//! - **`std`** (default) — enables full tree construction via [`tree::SparseMerkleTree`].
//! - Without `std` — only proof verification ([`proof::SmtProof`]) is available,
//!   suitable for `no_std` environments and ZK guest programs.
//!
//! ## Proof compression
//!
//! A 32-byte (256-bit) bitmap compresses away empty-subtree siblings.
//! Only non-empty siblings are stored, reducing typical proof size from
//! 8 KiB (256 × 32 bytes) to ~32 + 32 × log₂(N) bytes.
//!
//! ## Node hashing
//!
//! The tree is generic over the internal node hasher `H: SmtHasher`. Callers
//! supply the concrete hasher type (e.g., `SeqCommitActiveNode`). Leaf hashes
//! are opaque `Hash` values inserted externally.

#![no_std]

extern crate alloc;
extern crate core;

#[cfg(feature = "std")]
extern crate std;

pub mod proof;
#[cfg(feature = "std")]
pub mod tree;

use alloc::boxed::Box;
use kaspa_hashes::{Hash, Hasher, ZERO_HASH};

/// Depth of the sparse Merkle tree (number of levels from root to leaf).
pub const DEPTH: usize = 256;

/// Compute the hash of an internal node from its left and right children.
///
/// Uses the provided hasher `H` for domain-separated hashing:
/// `H(left_bytes || right_bytes)`.
#[inline]
pub fn hash_node<H: Hasher>(left: Hash, right: Hash) -> Hash {
    let mut hasher = H::default();
    hasher.update(left).update(right);
    hasher.finalize()
}

/// Extract bit `d` of a key using big-endian bit ordering.
///
/// - Bit 0 = MSB of byte 0 (used at the root-level split)
/// - Bit 255 = LSB of byte 31 (used at the leaf-level split)
///
/// Returns `true` if the bit is set (right branch), `false` if clear (left branch).
#[inline]
pub fn bit_at(key: &Hash, d: usize) -> bool {
    key.as_slice()[d / 8] & (0x80 >> (d % 8)) != 0
}

/// Compute the root hash of a completely empty SMT.
///
/// Performs [`DEPTH`] (256) hash operations. For repeated use, prefer
/// [`compute_empty_hashes`] which returns all levels at once.
pub fn empty_root<H: Hasher>() -> Hash {
    compute_empty_hashes::<H>()[DEPTH]
}

/// Precompute empty subtree hashes for every level of the tree.
///
/// Returns a boxed array of 257 hashes indexed by *level* (height from leaf):
/// - `[0]` = leaf level = [`ZERO_HASH`] (the canonical empty leaf)
/// - `[i]` = `hash_node::<H>([i-1], [i-1])`
/// - `[DEPTH]` = root of a completely empty tree = [`empty_root`]
///
/// The result is heap-allocated (8 KiB+) to avoid large stack moves.
pub fn compute_empty_hashes<H: Hasher>() -> Box<[Hash; DEPTH + 1]> {
    let mut result = Box::new([ZERO_HASH; DEPTH + 1]);
    for i in 1..=DEPTH {
        result[i] = hash_node::<H>(result[i - 1], result[i - 1]);
    }
    result
}

/// Trait for hashers that can be used with [`tree::SparseMerkleTree`].
///
/// Provides precomputed empty subtree hashes for efficient tree operations.
/// Implementations for all BLAKE3-based hashers in `kaspa-hashes` are
/// generated at build time via `build.rs`.
pub trait SmtHasher: Hasher {
    /// Indexed by level (height from leaf):
    /// - `[0]` = `ZERO_HASH` (canonical empty leaf)
    /// - `[i]` = `hash_node(empty[i-1], empty[i-1])`
    /// - `[DEPTH]` = root of a completely empty tree
    const EMPTY_HASHES: [Hash; DEPTH + 1];
}

// Build-time generated `SmtHasher` impls for known hashers.
include!(concat!(env!("OUT_DIR"), "/empty_hashes_generated.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::SeqCommitActiveNode;

    type H = SeqCommitActiveNode;

    #[test]
    fn test_key_bit_extraction() {
        // All zeros: every bit is false
        let zero_key = Hash::from_bytes([0u8; 32]);
        for d in 0..256 {
            assert!(!bit_at(&zero_key, d), "zero key bit {d}");
        }

        // All ones: every bit is true
        let ones_key = Hash::from_bytes([0xFF; 32]);
        for d in 0..256 {
            assert!(bit_at(&ones_key, d), "ones key bit {d}");
        }

        // 0x80 in byte 0 = bit 0 is 1, bits 1-7 are 0
        let mut bytes = [0u8; 32];
        bytes[0] = 0x80;
        let key = Hash::from_bytes(bytes);
        assert!(bit_at(&key, 0));
        assert!(!bit_at(&key, 1));
        assert!(!bit_at(&key, 7));

        // 0x01 in byte 31 = bit 255 is 1, bit 254 is 0
        let mut bytes = [0u8; 32];
        bytes[31] = 0x01;
        let key = Hash::from_bytes(bytes);
        assert!(!bit_at(&key, 254));
        assert!(bit_at(&key, 255));

        // 0xA5 = 10100101 in byte 0: bits 0,2,5,7 are 1
        let mut bytes = [0u8; 32];
        bytes[0] = 0xA5;
        let key = Hash::from_bytes(bytes);
        assert!(bit_at(&key, 0));
        assert!(!bit_at(&key, 1));
        assert!(bit_at(&key, 2));
        assert!(!bit_at(&key, 3));
        assert!(!bit_at(&key, 4));
        assert!(bit_at(&key, 5));
        assert!(!bit_at(&key, 6));
        assert!(bit_at(&key, 7));
    }

    #[test]
    fn test_empty_root_deterministic() {
        let r1 = empty_root::<H>();
        let r2 = empty_root::<H>();
        assert_eq!(r1, r2);
        assert_ne!(r1, ZERO_HASH, "empty root should not be ZERO_HASH");
    }

    #[test]
    fn test_compute_empty_hashes_consistency() {
        let table = compute_empty_hashes::<H>();

        // Level 0 is ZERO_HASH (empty leaf)
        assert_eq!(table[0], ZERO_HASH);

        // Level 1 = H(ZERO, ZERO)
        assert_eq!(table[1], hash_node::<H>(ZERO_HASH, ZERO_HASH));

        // Level 2 = H(level1, level1)
        assert_eq!(table[2], hash_node::<H>(table[1], table[1]));

        // Top level = empty_root
        assert_eq!(table[DEPTH], empty_root::<H>());

        // All levels are distinct
        for i in 0..=DEPTH {
            for j in (i + 1)..=DEPTH {
                assert_ne!(table[i], table[j], "empty_hashes[{i}] == empty_hashes[{j}]");
            }
        }
    }

    #[test]
    fn test_compute_empty_hashes_is_boxed() {
        // Verify the result is heap-allocated (Box) and doesn't blow the stack
        let table = compute_empty_hashes::<H>();
        assert_eq!(core::mem::size_of_val(&table), core::mem::size_of::<usize>());
    }

    #[test]
    fn test_smt_hasher_matches_runtime_computation() {
        let computed = compute_empty_hashes::<H>();
        for i in 0..=DEPTH {
            assert_eq!(H::EMPTY_HASHES[i], computed[i], "build.rs vs runtime mismatch at level {i}");
        }
    }

    #[test]
    fn test_smt_hasher_empty_root() {
        assert_eq!(H::EMPTY_HASHES[0], ZERO_HASH);
        assert_eq!(H::EMPTY_HASHES[DEPTH], empty_root::<H>());
    }
}
