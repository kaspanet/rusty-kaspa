//! Streaming (stack-based) Merkle tree builder — no heap allocation.
//!
//! Produces the same root as
//! [`calc_merkle_root_with_hasher::<H, true>`](super::calc_merkle_root_with_hasher)
//! but processes leaves one at a time with O(1) amortised work and
//! O(log N) stack space, all without heap allocation.

use kaspa_hashes::{Hash, Hasher, ZERO_HASH};

/// Maximum stack depth — supports up to 2^32 leaves.
const MAX_STACK: usize = 32;

/// Streaming Merkle tree builder parameterized by a [`Hasher`].
///
/// ```text
/// add_leaf(h1)  →  stack: [(0,h1)]
/// add_leaf(h2)  →  stack: [(1, branch(h1,h2))]
/// add_leaf(h3)  →  stack: [(1, branch(h1,h2)), (0,h3)]
/// finalize()    →  branch(branch(h1,h2), branch(h3,ZERO))
/// ```
pub struct StreamingMerkleBuilder<H: Hasher> {
    stack: [(u32, Hash); MAX_STACK],
    stack_len: usize,
    leaf_count: u32,
    _h: core::marker::PhantomData<H>,
}

impl<H: Hasher> Default for StreamingMerkleBuilder<H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: Hasher> StreamingMerkleBuilder<H> {
    pub fn new() -> Self {
        Self { stack: [(0, ZERO_HASH); MAX_STACK], stack_len: 0, leaf_count: 0, _h: core::marker::PhantomData }
    }

    /// Add a pre-hashed leaf.
    pub fn add_leaf(&mut self, hash: Hash) {
        let mut level = 0u32;
        let mut current = hash;

        while self.stack_len > 0 {
            let (top_level, top_hash) = self.stack[self.stack_len - 1];
            if top_level != level {
                break;
            }
            self.stack_len -= 1;
            current = Self::branch(&top_hash, &current);
            level += 1;
        }

        self.stack[self.stack_len] = (level, current);
        self.stack_len += 1;
        self.leaf_count += 1;
    }

    /// Number of leaves added so far.
    pub fn leaf_count(&self) -> u32 {
        self.leaf_count
    }

    /// Finalize and return the Merkle root.
    ///
    /// Pads incomplete subtrees with [`ZERO_HASH`], matching the
    /// convention used by [`calc_merkle_root_with_hasher`](super::calc_merkle_root_with_hasher).
    pub fn finalize(self) -> Hash {
        if self.leaf_count == 0 {
            return ZERO_HASH;
        }

        if self.leaf_count == 1 {
            return Self::branch(&self.stack[0].1, &ZERO_HASH);
        }

        // Stack represents a binary decomposition of the leaf count.
        // Process from right (top of stack) to left, padding as needed.
        let mut result_hash = ZERO_HASH;
        let mut result_level = 0u32;
        let mut first = true;

        for i in (0..self.stack_len).rev() {
            let (level, hash) = self.stack[i];

            if first {
                result_hash = hash;
                result_level = level;
                first = false;
                continue;
            }

            // Pad result from its current level up to this node's level
            while result_level < level {
                result_hash = Self::branch(&result_hash, &ZERO_HASH);
                result_level += 1;
            }

            // Merge: this node (left) with padded result (right)
            result_hash = Self::branch(&hash, &result_hash);
            result_level += 1;
        }

        result_hash
    }

    #[inline]
    fn branch(left: &Hash, right: &Hash) -> Hash {
        let mut hasher = H::default();
        hasher.update(left).update(right);
        hasher.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc_merkle_root_with_hasher;
    use alloc::vec::Vec;
    use kaspa_hashes::{HasherBase, SeqCommitMerkleBranch, TransactionHash};

    type SeqStreamBuilder = StreamingMerkleBuilder<SeqCommitMerkleBranch>;

    fn make_hash(data: &[u8]) -> Hash {
        let mut hasher = TransactionHash::new();
        hasher.update(data);
        hasher.finalize()
    }

    fn seq_root(hashes: impl ExactSizeIterator<Item = Hash>) -> Hash {
        calc_merkle_root_with_hasher::<SeqCommitMerkleBranch, true>(hashes)
    }

    #[test]
    fn empty_tree() {
        let builder = SeqStreamBuilder::new();
        assert_eq!(builder.finalize(), ZERO_HASH);
        assert_eq!(seq_root(core::iter::empty()), ZERO_HASH);
    }

    #[test]
    fn single_leaf() {
        let h = make_hash(b"leaf");
        let mut builder = SeqStreamBuilder::new();
        builder.add_leaf(h);
        assert_eq!(builder.finalize(), seq_root(core::iter::once(h)));
    }

    #[test]
    fn two_leaves() {
        let hashes: Vec<Hash> = (0..2).map(|i| make_hash(&[i])).collect();
        let mut builder = SeqStreamBuilder::new();
        for &h in &hashes {
            builder.add_leaf(h);
        }
        assert_eq!(builder.finalize(), seq_root(hashes.into_iter()));
    }

    #[test]
    fn various_sizes() {
        for count in [0, 1, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100, 256] {
            let hashes: Vec<Hash> = (0..count).map(|i: i32| make_hash(&i.to_le_bytes())).collect();
            let expected = seq_root(hashes.iter().copied());

            let mut builder = SeqStreamBuilder::new();
            for &h in &hashes {
                builder.add_leaf(h);
            }
            assert_eq!(builder.finalize(), expected, "mismatch at count={count}");
        }
    }

    #[test]
    fn leaf_count_tracking() {
        let mut builder = SeqStreamBuilder::new();
        assert_eq!(builder.leaf_count(), 0);
        builder.add_leaf(make_hash(b"a"));
        assert_eq!(builder.leaf_count(), 1);
        builder.add_leaf(make_hash(b"b"));
        assert_eq!(builder.leaf_count(), 2);
    }

    #[test]
    fn order_matters() {
        let h1 = make_hash(b"h1");
        let h2 = make_hash(b"h2");

        let mut b1 = SeqStreamBuilder::new();
        b1.add_leaf(h1);
        b1.add_leaf(h2);

        let mut b2 = SeqStreamBuilder::new();
        b2.add_leaf(h2);
        b2.add_leaf(h1);

        assert_ne!(b1.finalize(), b2.finalize());
    }
}
