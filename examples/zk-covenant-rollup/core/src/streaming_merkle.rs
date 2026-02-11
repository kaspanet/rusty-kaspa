//! Generic streaming Merkle tree builder — no heap allocation.
//!
//! Uses a fixed-size stack to build the tree incrementally. Parameterized
//! by a [`MerkleHashOps`] trait that supplies the branch hash and empty
//! subtree hash functions, so the same builder works for seq-commitment
//! trees (blake3 + zero padding) and permission trees (SHA256 + PermEmpty
//! padding).

/// Maximum stack depth — supports up to 2^32 leaves.
const MAX_STACK: usize = 32;

/// Operations that define a particular Merkle tree domain.
pub trait MerkleHashOps {
    /// Hash two children into a parent.
    fn branch(left: &[u32; 8], right: &[u32; 8]) -> [u32; 8];

    /// Empty subtree hash at the given level.
    ///
    /// Level 0 = the empty *leaf* sentinel.
    /// Level n = `branch(empty(n-1), empty(n-1))`.
    fn empty_subtree(level: usize) -> [u32; 8];
}

/// Streaming (stack-based) Merkle tree builder.
///
/// Processes leaves one at a time with O(1) amortised work per leaf and
/// O(log N) stack space — all on the program stack (no heap).
pub struct StreamingMerkle<H: MerkleHashOps> {
    stack: [(u32, [u32; 8]); MAX_STACK],
    stack_len: usize,
    leaf_count: u32,
    _h: core::marker::PhantomData<H>,
}

impl<H: MerkleHashOps> Default for StreamingMerkle<H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: MerkleHashOps> StreamingMerkle<H> {
    pub fn new() -> Self {
        Self { stack: [(0, [0u32; 8]); MAX_STACK], stack_len: 0, leaf_count: 0, _h: core::marker::PhantomData }
    }

    /// Add a pre-hashed leaf.
    pub fn add_leaf(&mut self, hash: [u32; 8]) {
        let mut level = 0u32;
        let mut current = hash;

        while self.stack_len > 0 {
            let (top_level, top_hash) = self.stack[self.stack_len - 1];
            if top_level != level {
                break;
            }
            self.stack_len -= 1;
            current = H::branch(&top_hash, &current);
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
    /// Pads incomplete subtrees with the domain-specific empty subtree
    /// hashes.
    pub fn finalize(self) -> [u32; 8] {
        if self.leaf_count == 0 {
            return H::empty_subtree(0);
        }

        if self.leaf_count == 1 {
            return H::branch(&self.stack[0].1, &H::empty_subtree(0));
        }

        // Stack represents a binary decomposition of the leaf count.
        // Process from right (top of stack) to left, padding as needed.
        let mut result_hash = [0u32; 8];
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
                result_hash = H::branch(&result_hash, &H::empty_subtree(result_level as usize));
                result_level += 1;
            }

            // Merge: this node (left) with padded result (right)
            result_hash = H::branch(&hash, &result_hash);
            result_level += 1;
        }

        result_hash
    }
}
