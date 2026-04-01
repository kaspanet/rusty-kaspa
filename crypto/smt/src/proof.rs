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

use crate::store::{BranchKey, CollapsedLeaf};
/// Branch cache for proof verification short-circuiting.
pub type ProofBranchCache = alloc::collections::BTreeMap<BranchKey, Hash>;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofTerminal {
    Full,
    Collapsed { depth: u8 },
    CollapsedOther { depth: u8, leaf: CollapsedLeaf },
}

impl ProofTerminal {
    const FULL_TAG: u8 = 0;
    const COLLAPSED_TAG: u8 = 1;
    const COLLAPSED_OTHER_TAG: u8 = 2;

    fn depth(self) -> usize {
        match self {
            Self::Full => DEPTH,
            Self::Collapsed { depth } | Self::CollapsedOther { depth, .. } => depth as usize,
        }
    }
}

fn bitmap_clear_count_before(bitmap: &[u8; 32], terminal: ProofTerminal) -> usize {
    let limit = terminal.depth();
    (0..limit).filter(|&d| !is_empty_at_depth(bitmap, d)).count()
}

/// Compute the Merkle root from a proof.
/// If `cache` is provided, each branch is looked up first (skip hash_node if found)
/// and newly computed branches are inserted.
fn compute_root_inner<H: SmtHasher>(
    bitmap: &[u8; 32],
    siblings: &[Hash],
    terminal: ProofTerminal,
    key: &Hash,
    leaf_hash: Option<Hash>,
    mut cache: Option<&mut ProofBranchCache>,
) -> Result<Hash, SmtProofError> {
    let expected = bitmap_clear_count_before(bitmap, terminal);
    if siblings.len() != expected {
        return Err(SmtProofError::SiblingCountMismatch { expected, actual: siblings.len() });
    }

    let mut current = match (terminal, leaf_hash) {
        (ProofTerminal::CollapsedOther { leaf, .. }, None) if leaf.lane_key != *key => {
            hash_node::<H::CollapsedHasher>(leaf.lane_key, leaf.leaf_hash)
        }
        (ProofTerminal::CollapsedOther { .. }, None) => ZERO_HASH,
        (_, Some(leaf_hash)) => hash_node::<H::CollapsedHasher>(*key, leaf_hash),
        (_, None) => ZERO_HASH,
    };
    let mut sib_idx = siblings.len();
    let limit = terminal.depth();

    // d ranges from limit-1 down to 0. limit <= DEPTH (256).
    // EMPTY_HASHES[DEPTH - 1 - d]: d < DEPTH, so index is in 0..=255 (safe).
    // BranchKey::new(d as u8, ..): d < 256, so u8 cast is safe.
    for d in (0..limit).rev() {
        let sibling = if is_empty_at_depth(bitmap, d) {
            H::EMPTY_HASHES[DEPTH - 1 - d]
        } else {
            sib_idx -= 1;
            siblings[sib_idx]
        };

        let (left, right) = if bit_at(key, d) { (sibling, current) } else { (current, sibling) };

        current = if let Some(ref mut cache) = cache {
            let bk = BranchKey::new(d as u8, key);
            if let Some(&cached_hash) = cache.get(&bk) {
                cached_hash
            } else {
                let node_hash = hash_node::<H>(left, right);
                cache.insert(bk, node_hash);
                node_hash
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
    pub terminal: ProofTerminal,
    _marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> SmtProof<'a> {
    pub fn compute_root<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>) -> Result<Hash, SmtProofError> {
        compute_root_inner::<H>(self.bitmap, self.siblings, self.terminal, key, leaf_hash, None)
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
        cache: &mut ProofBranchCache,
    ) -> Result<bool, SmtProofError> {
        let computed = compute_root_inner::<H>(self.bitmap, self.siblings, self.terminal, key, leaf_hash, Some(cache))?;
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
    pub terminal: ProofTerminal,
}

impl OwnedSmtProof {
    /// Parse from wire format: `bitmap[32] || terminal_tag[1] || terminal_payload || siblings[N * 32]`.
    pub fn from_bytes(data: &[u8]) -> Result<Self, SmtProofError> {
        let Some((&bitmap, rem)) = data.split_first_chunk::<32>() else {
            return Err(SmtProofError::SiblingCountMismatch { expected: 0, actual: 0 });
        };
        let Some((&tag, rem)) = rem.split_first() else {
            return Err(SmtProofError::SiblingCountMismatch { expected: bitmap_clear_count(&bitmap), actual: 0 });
        };
        let (terminal, sibling_bytes) = match tag {
            ProofTerminal::FULL_TAG => (ProofTerminal::Full, rem),
            ProofTerminal::COLLAPSED_TAG => {
                let Some((&depth, rem)) = rem.split_first() else {
                    return Err(SmtProofError::SiblingCountMismatch { expected: 0, actual: 0 });
                };
                (ProofTerminal::Collapsed { depth }, rem)
            }
            ProofTerminal::COLLAPSED_OTHER_TAG => {
                let Some((&depth, rem)) = rem.split_first() else {
                    return Err(SmtProofError::SiblingCountMismatch { expected: 0, actual: 0 });
                };
                let Some((&lane_key, rem)) = rem.split_first_chunk::<32>() else {
                    return Err(SmtProofError::SiblingCountMismatch { expected: 0, actual: 0 });
                };
                let Some((&leaf_hash, rem)) = rem.split_first_chunk::<32>() else {
                    return Err(SmtProofError::SiblingCountMismatch { expected: 0, actual: 0 });
                };
                (
                    ProofTerminal::CollapsedOther {
                        depth,
                        leaf: CollapsedLeaf { lane_key: Hash::from_bytes(lane_key), leaf_hash: Hash::from_bytes(leaf_hash) },
                    },
                    rem,
                )
            }
            _ => return Err(SmtProofError::SiblingCountMismatch { expected: bitmap_clear_count(&bitmap), actual: rem.len() / 32 }),
        };
        let (siblings, rem) = sibling_bytes.as_chunks::<32>();
        if !rem.is_empty() {
            return Err(SmtProofError::SiblingCountMismatch {
                expected: bitmap_clear_count_before(&bitmap, terminal),
                actual: sibling_bytes.len() / 32,
            });
        }
        let expected = bitmap_clear_count_before(&bitmap, terminal);
        if siblings.len() != expected {
            return Err(SmtProofError::SiblingCountMismatch { expected, actual: siblings.len() });
        }
        let siblings = siblings.iter().copied().map(Hash::from_bytes).collect::<Vec<_>>();
        Ok(Self { bitmap, siblings, terminal })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let terminal_len = match self.terminal {
            ProofTerminal::Full => 1,
            ProofTerminal::Collapsed { .. } => 2,
            ProofTerminal::CollapsedOther { .. } => 66,
        };
        let mut out = Vec::with_capacity(32 + terminal_len + self.siblings.len() * 32);
        out.extend_from_slice(&self.bitmap);
        match self.terminal {
            ProofTerminal::Full => out.push(ProofTerminal::FULL_TAG),
            ProofTerminal::Collapsed { depth } => {
                out.push(ProofTerminal::COLLAPSED_TAG);
                out.push(depth);
            }
            ProofTerminal::CollapsedOther { depth, leaf } => {
                out.push(ProofTerminal::COLLAPSED_OTHER_TAG);
                out.push(depth);
                out.extend_from_slice(leaf.lane_key.as_bytes().as_slice());
                out.extend_from_slice(leaf.leaf_hash.as_bytes().as_slice());
            }
        }
        for sibling in &self.siblings {
            out.extend_from_slice(sibling.as_bytes().as_slice());
        }
        out
    }

    pub fn as_proof(&self) -> SmtProof<'_> {
        SmtProof { bitmap: &self.bitmap, siblings: &self.siblings, terminal: self.terminal, _marker: core::marker::PhantomData }
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
