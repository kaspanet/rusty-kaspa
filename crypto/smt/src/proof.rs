//! Sparse Merkle Tree proof types and `no_std` verification.
//!
//! # Proof format
//!
//! A proof consists of sibling hashes compressed via a 32-byte bitmap.
//! When a sibling equals the canonical empty subtree hash for its level,
//! the bitmap bit is set and the sibling is omitted. This typically reduces
//! proof size from 8 KiB to a few hundred bytes.
//!
//! # SLO (Suffix-Only Leaf) optimization and [`ProofTerminal`]
//!
//! This SMT stores single-leaf subtrees as a single [`Collapsed`](crate::store::Node::Collapsed)
//! node instead of a full chain of internal nodes down to depth 255. As a result,
//! proof traversal can stop early when it reaches a collapsed subtree.
//!
//! [`ProofTerminal`] records *where* and *how* a proof stopped:
//! - [`Full`](ProofTerminal::Full) — traversal reached the leaf level (depth 256); classic SMT proof.
//! - [`Collapsed`](ProofTerminal::Collapsed) — traversal stopped at a collapsed subtree that
//!   contains the queried key (inclusion proof with early termination).
//! - [`CollapsedOther`](ProofTerminal::CollapsedOther) — traversal stopped at a collapsed subtree
//!   containing a *different* key; this is a non-inclusion witness that carries the conflicting
//!   leaf so the verifier can reconstruct the subtree hash.
//!
//! Without this information the verifier cannot know:
//! - the depth at which to begin hashing upward,
//! - whether the proof is an inclusion or non-inclusion witness, or
//! - the hash of a foreign leaf occupying the collapsed subtree.
//!
//! # Proof types
//!
//! - [`SmtProof`] — borrowed view, zero-alloc, usable in `no_std` / ZK.
//! - [`OwnedSmtProof`] — owned, delegates to `SmtProof` via `as_proof()`.

use alloc::vec::Vec;
use kaspa_hashes::{Hash, ZERO_HASH};

use crate::store::{BranchKey, CollapsedLeaf};
/// Cache of already-computed branch hashes, keyed by `(depth, key_prefix)`.
///
/// Used by [`SmtProof::verify_cached`] to skip redundant `hash_node` calls when
/// verifying multiple proofs against the same tree root. Upper branches are
/// shared across proofs, so this can significantly reduce hashing work.
pub type ProofBranchCache = alloc::collections::BTreeMap<BranchKey, Hash>;
use crate::{DEPTH, SmtHasher, bit_at, hash_node};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SmtProofError {
    #[error("sibling count mismatch: bitmap implies {expected} non-empty siblings, but got {actual}")]
    SiblingCountMismatch { expected: usize, actual: usize },
}

/// Returns `true` if the sibling at depth `d` is empty (its bitmap bit is set),
/// meaning it equals the canonical empty-subtree hash and was elided from the proof.
#[inline]
fn is_empty_at_depth(bitmap: &[u8; 32], d: usize) -> bool {
    bitmap[d / 8] & (1 << (d % 8)) != 0
}

/// Count non-empty siblings across all 256 depths (ignores terminal).
/// Used only during deserialization before the terminal is known.
fn bitmap_clear_count(bitmap: &[u8; 32]) -> usize {
    DEPTH - bitmap.iter().map(|b| b.count_ones() as usize).sum::<usize>()
}

/// Describes how proof traversal terminated.
///
/// Because this SMT uses the SLO (Suffix-Only Leaf) optimization, a subtree with
/// exactly one leaf is stored as a single [`Collapsed`](crate::store::Node::Collapsed)
/// node rather than a full chain of 256 internal nodes. Proof generation can therefore
/// stop before reaching the leaf level.
///
/// The verifier needs this information for three reasons:
/// 1. **Loop bound** — `depth()` tells the verifier where to start hashing upward.
/// 2. **Sibling count** — only siblings at depths `0..depth()` are present in the proof.
/// 3. **Semantic distinction** — `CollapsedOther` changes the initial hash seed
///    (the verifier starts from the foreign leaf hash, not the queried key).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofTerminal {
    /// Proof descends the full 256 levels to the leaf layer. This is the classic
    /// SMT proof where every level has a real internal node.
    Full,

    /// Proof stopped at `depth` because the queried key lives inside a collapsed
    /// (single-leaf) subtree. This is an inclusion proof with early termination.
    /// The verifier seeds with `hash(collapsed_hasher, queried_key, leaf_hash)` and
    /// hashes upward from `depth - 1` to the root.
    Collapsed { depth: u8 },

    /// Proof stopped at `depth` because a *different* key occupies the collapsed subtree.
    /// This is a non-inclusion witness: the queried key is absent, and `leaf` is the
    /// conflicting [`CollapsedLeaf`] that the verifier needs to reconstruct the subtree hash.
    CollapsedOther { depth: u8, leaf: CollapsedLeaf },
}

impl ProofTerminal {
    /// Wire-format tag for [`Full`](Self::Full).
    const FULL_TAG: u8 = 0;
    /// Wire-format tag for [`Collapsed`](Self::Collapsed).
    const COLLAPSED_TAG: u8 = 1;
    /// Wire-format tag for [`CollapsedOther`](Self::CollapsedOther).
    const COLLAPSED_OTHER_TAG: u8 = 2;

    /// The tree depth at which this proof terminates.
    ///
    /// - `Full` → `DEPTH` (256): the proof covers all levels.
    /// - `Collapsed` / `CollapsedOther` → the depth of the collapsed node.
    ///
    /// The verifier uses this to determine:
    /// - how many siblings to expect (only levels `0..depth()` are present), and
    /// - the starting level for the upward hashing loop (`depth() - 1`).
    fn depth(self) -> usize {
        match self {
            Self::Full => DEPTH,
            Self::Collapsed { depth } | Self::CollapsedOther { depth, .. } => depth as usize,
        }
    }
}

/// Count non-empty siblings in `bitmap` up to (but not including) the terminal depth.
///
/// Only levels `0..terminal.depth()` contribute siblings to the proof. Levels at or
/// beyond the terminal depth are either inside the collapsed subtree or absent, so
/// their bitmap bits are ignored for sibling-count validation.
fn bitmap_clear_count_before(bitmap: &[u8; 32], terminal: ProofTerminal) -> usize {
    let limit = terminal.depth();
    (0..limit).filter(|&d| !is_empty_at_depth(bitmap, d)).count()
}

/// Reconstruct the Merkle root from a proof, optionally using a branch cache.
///
/// # Terminal-dependent initial state
///
/// The starting hash (`current`) depends on [`ProofTerminal`]:
///
/// | Terminal | `leaf_hash` | Initial `current` |
/// |---|---|---|
/// | `CollapsedOther` | `None`, different key | `hash(collapsed, foreign_key, foreign_leaf)` — non-inclusion witness |
/// | `CollapsedOther` | `None`, same key | `ZERO_HASH` — proves non-membership inside that subtree |
/// | any | `Some(lh)` | `hash(collapsed, queried_key, lh)` — inclusion proof |
/// | any | `None` | `ZERO_HASH` — non-inclusion (empty subtree) |
///
/// After seeding `current`, the function hashes upward from `terminal.depth() - 1`
/// to the root (depth 0), consuming siblings in reverse bitmap order.
///
/// If `cache` is provided, each intermediate branch node is looked up before hashing;
/// cache hits skip the hash computation and new results are inserted.
fn compute_root_inner<H: SmtHasher>(
    bitmap: &[u8; 32],
    siblings: &[Hash],
    terminal: ProofTerminal,
    key: &Hash,
    leaf_hash: Option<Hash>,
    mut cache: Option<&mut ProofBranchCache>,
) -> Result<Hash, SmtProofError> {
    // Validate that the sibling count matches the bitmap up to the terminal depth.
    let expected = bitmap_clear_count_before(bitmap, terminal);
    if siblings.len() != expected {
        return Err(SmtProofError::SiblingCountMismatch { expected, actual: siblings.len() });
    }

    // Seed the initial hash based on the terminal variant and queried leaf.
    let mut current = match (terminal, leaf_hash) {
        // Non-inclusion: collapsed subtree holds a different key → start from foreign leaf hash.
        (ProofTerminal::CollapsedOther { leaf, .. }, None) if leaf.lane_key != *key => {
            hash_node::<H::CollapsedHasher>(leaf.lane_key, leaf.leaf_hash)
        }
        // Edge case: CollapsedOther but the key matches → treat as empty (non-membership).
        (ProofTerminal::CollapsedOther { .. }, None) => ZERO_HASH,
        // Inclusion proof: hash the queried key with its leaf value.
        (_, Some(leaf_hash)) => hash_node::<H::CollapsedHasher>(*key, leaf_hash),
        // Non-inclusion: subtree is empty.
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

/// Borrowed, zero-copy compressed proof for a 256-bit Sparse Merkle Tree.
///
/// Suitable for `no_std` / ZK contexts since it requires no allocation.
/// See [`OwnedSmtProof`] for the owned variant and [`OwnedSmtProof::as_proof`] for conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmtProof<'a> {
    /// 32-byte bitmap (256 bits). A set bit at position `d` means the sibling at depth `d`
    /// equals the canonical empty-subtree hash and is therefore omitted from `siblings`.
    pub bitmap: &'a [u8; 32],
    /// Non-empty sibling hashes, in ascending depth order, up to [`terminal.depth()`](ProofTerminal::depth).
    pub siblings: &'a [Hash],
    /// How proof traversal ended. Determines verification loop bounds and initial hash seed.
    pub terminal: ProofTerminal,
    _marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> SmtProof<'a> {
    /// Reconstruct the Merkle root that this proof implies for `key` with an optional `leaf_hash`.
    ///
    /// - `leaf_hash = Some(h)` — inclusion proof: the key is present with value `h`.
    /// - `leaf_hash = None` — non-inclusion proof: the key is absent.
    ///
    /// The reconstruction respects [`self.terminal`](ProofTerminal) to determine the starting
    /// hash and loop depth. Returns an error if the sibling count doesn't match the bitmap.
    pub fn compute_root<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>) -> Result<Hash, SmtProofError> {
        compute_root_inner::<H>(self.bitmap, self.siblings, self.terminal, key, leaf_hash, None)
    }

    /// Verify that the proof is consistent with the given `root`.
    ///
    /// Equivalent to `self.compute_root(key, leaf_hash)? == root`.
    pub fn verify<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>, root: Hash) -> Result<bool, SmtProofError> {
        Ok(self.compute_root::<H>(key, leaf_hash)? == root)
    }

    /// Verify the proof while populating `cache` with intermediate branch hashes.
    ///
    /// Branches already in the cache are reused (skipping the hash computation),
    /// and newly computed branches are inserted. This is useful when verifying
    /// many proofs against the same tree, since upper branches are shared.
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

    /// Number of non-empty (explicitly stored) sibling hashes in this proof.
    pub fn non_empty_count(&self) -> usize {
        self.siblings.len()
    }

    /// Number of empty (bitmap-elided) sibling positions in this proof.
    pub fn empty_count(&self) -> usize {
        DEPTH - self.siblings.len()
    }
}

/// Owned compressed proof for a 256-bit Sparse Merkle Tree.
///
/// This is the serializable/deserializable form of a proof. Use [`as_proof`](Self::as_proof)
/// to obtain a borrowed [`SmtProof`] for verification.
///
/// # Wire format
///
/// ```text
/// bitmap[32] || terminal_tag[1] || terminal_payload || siblings[N × 32]
/// ```
///
/// - `Full`: tag `0`, no payload.
/// - `Collapsed`: tag `1`, payload = `depth[1]`.
/// - `CollapsedOther`: tag `2`, payload = `depth[1] || lane_key[32] || leaf_hash[32]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedSmtProof {
    /// 32-byte bitmap. See [`SmtProof::bitmap`].
    pub bitmap: [u8; 32],
    /// Non-empty sibling hashes. See [`SmtProof::siblings`].
    pub siblings: Vec<Hash>,
    /// How proof traversal ended. See [`ProofTerminal`].
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

    /// Serialize to wire format: `bitmap[32] || terminal_tag[1] || terminal_payload || siblings[N × 32]`.
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

    /// Borrow as a [`SmtProof`] for zero-copy verification.
    pub fn as_proof(&self) -> SmtProof<'_> {
        SmtProof { bitmap: &self.bitmap, siblings: &self.siblings, terminal: self.terminal, _marker: core::marker::PhantomData }
    }

    /// Reconstruct the Merkle root. Delegates to [`SmtProof::compute_root`].
    pub fn compute_root<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>) -> Result<Hash, SmtProofError> {
        self.as_proof().compute_root::<H>(key, leaf_hash)
    }

    /// Verify against `root`. Delegates to [`SmtProof::verify`].
    pub fn verify<H: SmtHasher>(&self, key: &Hash, leaf_hash: Option<Hash>, root: Hash) -> Result<bool, SmtProofError> {
        self.as_proof().verify::<H>(key, leaf_hash, root)
    }

    /// Number of non-empty (explicitly stored) sibling hashes.
    pub fn non_empty_count(&self) -> usize {
        self.siblings.len()
    }

    /// Number of empty (bitmap-elided) sibling positions.
    pub fn empty_count(&self) -> usize {
        DEPTH - self.siblings.len()
    }
}
