use crate::domain_to_key;
use alloc::vec;

const DOMAIN_SEP: &[u8] = b"SeqCommitmentMerkleBranchHash";
const KEY: [u8; blake3::KEY_LEN] = domain_to_key(DOMAIN_SEP);

const ZERO_HASH: [u32; 8] = [0u32; 8];
pub fn merkle_hash(left: &[u32; 8], right: &[u32; 8], mut hasher: blake3::Hasher) -> [u32; 8] {
    hasher.update(bytemuck::bytes_of(left));
    hasher.update(bytemuck::bytes_of(right));

    let mut out = [0u32; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(hasher.finalize().as_bytes());
    out
}

pub fn calc_merkle_root(mut hashes: impl ExactSizeIterator<Item = [u32; 8]>) -> [u32; 8] {
    match hashes.len() {
        0 => return cold_path_empty(),
        1 => return merkle_hash(&hashes.next().unwrap(), &ZERO_HASH, blake3::Hasher::new_keyed(&KEY)),
        _ => {}
    }
    let next_pot = hashes.len().next_power_of_two();
    let vec_len = 2 * next_pot - 1;

    let mut merkles = vec![None; vec_len];
    for (i, hash) in hashes.enumerate() {
        merkles[i] = Some(hash);
    }
    let mut offset = next_pot;
    for i in (0..vec_len - 1).step_by(2) {
        if merkles[i].is_none() {
            merkles[offset] = None;
        } else {
            merkles[offset] =
                Some(merkle_hash(&merkles[i].unwrap(), &merkles[i + 1].unwrap_or(ZERO_HASH), blake3::Hasher::new_keyed(&KEY)));
        }
        offset += 1
    }
    merkles.last().unwrap().unwrap()
}

#[inline(never)]
#[cold]
fn cold_path_empty() -> [u32; 8] {
    ZERO_HASH
}

pub fn calc_accepted_id_merkle_root(
    selected_parent_accepted_id_merkle_root: &[u32; 8],
    accepted_id_merkle_root: &[u32; 8],
) -> [u32; 8] {
    merkle_hash(selected_parent_accepted_id_merkle_root, accepted_id_merkle_root, blake3::Hasher::new_keyed(&KEY))
}

pub fn seq_commitment_leaf(tx_id: &[u32; 8], tx_version: u16) -> [u32; 8] {
    const DOMAIN_SEP: &[u8] = b"SeqCommitmentMerkleLeafHash";
    const KEY: [u8; blake3::KEY_LEN] = domain_to_key(DOMAIN_SEP);

    let mut hasher = blake3::Hasher::new_keyed(&KEY);
    hasher.update(bytemuck::bytes_of(tx_id));
    hasher.update(&tx_version.to_le_bytes());
    let mut out = [0u32; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(hasher.finalize().as_bytes());
    out
}

/// Streaming merkle tree builder that requires no heap allocation.
/// Works by maintaining a stack of intermediate hashes as we process leaves.
pub struct StreamingMerkleBuilder {
    // Stack of (level, hash) pairs. Max depth for u32::MAX leaves is 32.
    stack: [(u32, [u32; 8]); 32],
    stack_len: usize,
    leaf_count: u32,
}

impl Default for StreamingMerkleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingMerkleBuilder {
    pub fn new() -> Self {
        Self { stack: [(0, ZERO_HASH); 32], stack_len: 0, leaf_count: 0 }
    }

    /// Add a leaf to the merkle tree
    pub fn add_leaf(&mut self, hash: [u32; 8]) {
        let mut level = 0u32;
        let mut current_hash = hash;

        // Merge with existing nodes at the same level
        while self.stack_len > 0 {
            let (top_level, top_hash) = self.stack[self.stack_len - 1];
            if top_level != level {
                break;
            }

            // Pop from stack and merge
            self.stack_len -= 1;
            current_hash = merkle_hash(&top_hash, &current_hash, blake3::Hasher::new_keyed(&KEY));
            level += 1;
        }

        // Push the result onto stack
        self.stack[self.stack_len] = (level, current_hash);
        self.stack_len += 1;
        self.leaf_count += 1;
    }

    /// Finalize and return the merkle root
    pub fn finalize(self) -> [u32; 8] {
        if self.leaf_count == 0 {
            return ZERO_HASH;
        }

        if self.leaf_count == 1 {
            return merkle_hash(&self.stack[0].1, &ZERO_HASH, blake3::Hasher::new_keyed(&KEY));
        }

        // The stack represents a binary number where each bit position corresponds to a level.
        // To build the final tree, we need to process from RIGHT to LEFT (high index to low),
        // padding and merging as we go.
        //
        // Example: 7 elements gives stack [(2, A), (1, B), (0, C)] where:
        // - A is at level 2 (covers 4 elements)
        // - B is at level 1 (covers 2 elements)
        // - C is at level 0 (covers 1 element)
        //
        // We build from right: C → pad to level 1 → merge with B → pad to level 2 → merge with A

        let mut result_hash = ZERO_HASH;
        let mut result_level = 0u32;
        let mut first = true;

        // Process from right to left (high index to low)
        for i in (0..self.stack_len).rev() {
            let (level, hash) = self.stack[i];

            if first {
                result_hash = hash;
                result_level = level;
                first = false;
                continue;
            }

            // Pad result hash from its current level up to the level of the current node
            while result_level < level {
                result_hash = merkle_hash(&result_hash, &ZERO_HASH, blake3::Hasher::new_keyed(&KEY));
                result_level += 1;
            }

            // Now merge: current node (left) with padded result (right)
            result_hash = merkle_hash(&hash, &result_hash, blake3::Hasher::new_keyed(&KEY));
            result_level += 1;
        }

        result_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    // Helper to create a hash from bytes for testing
    fn make_test_hash(data: &[u8]) -> [u32; 8] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(data);
        let hash = hasher.finalize();
        let mut out = [0u32; 8];
        bytemuck::bytes_of_mut(&mut out).copy_from_slice(hash.as_bytes());
        out
    }

    // Helper to convert [u32; 8] to kaspa_hashes::Hash
    fn to_kaspa_hash(hash: &[u32; 8]) -> kaspa_hashes::Hash {
        kaspa_hashes::Hash::from_bytes(bytemuck::cast_slice(bytemuck::bytes_of(hash)).try_into().unwrap())
    }

    #[test]
    fn test_empty_merkle_tree() {
        // Original implementation
        let root_orig = calc_merkle_root(core::iter::empty());
        assert_eq!(root_orig, ZERO_HASH);

        // Streaming implementation
        let builder = StreamingMerkleBuilder::new();
        let root_stream = builder.finalize();
        assert_eq!(root_stream, ZERO_HASH);

        assert_eq!(root_orig, root_stream, "Empty tree: original vs streaming");
    }

    #[test]
    fn test_single_entry() {
        let h1 = make_test_hash(b"entry1");

        // Original implementation
        let root_orig = calc_merkle_root(core::iter::once(h1));

        // Streaming implementation
        let mut builder = StreamingMerkleBuilder::new();
        builder.add_leaf(h1);
        let root_stream = builder.finalize();

        assert_eq!(root_orig, root_stream, "Single entry: original vs streaming");

        // Verify it's hash(h1, ZERO_HASH)
        let expected = merkle_hash(&h1, &ZERO_HASH, blake3::Hasher::new_keyed(&KEY));
        assert_eq!(root_orig, expected);
    }

    #[test]
    fn test_two_entries() {
        let h1 = make_test_hash(b"entry1");
        let h2 = make_test_hash(b"entry2");

        // Original implementation
        let root_orig = calc_merkle_root([h1, h2].into_iter());

        // Streaming implementation
        let mut builder = StreamingMerkleBuilder::new();
        builder.add_leaf(h1);
        builder.add_leaf(h2);
        let root_stream = builder.finalize();

        assert_eq!(root_orig, root_stream, "Two entries: original vs streaming");
    }

    #[test]
    fn test_three_entries() {
        let h1 = make_test_hash(b"entry1");
        let h2 = make_test_hash(b"entry2");
        let h3 = make_test_hash(b"entry3");

        // Original implementation
        let root_orig = calc_merkle_root([h1, h2, h3].into_iter());

        // Streaming implementation
        let mut builder = StreamingMerkleBuilder::new();
        builder.add_leaf(h1);
        builder.add_leaf(h2);
        builder.add_leaf(h3);
        let root_stream = builder.finalize();

        assert_eq!(root_orig, root_stream, "Three entries: original vs streaming");

        // Verify tree structure: hash(hash(h1, h2), hash(h3, ZERO))
        let left = merkle_hash(&h1, &h2, blake3::Hasher::new_keyed(&KEY));
        let right = merkle_hash(&h3, &ZERO_HASH, blake3::Hasher::new_keyed(&KEY));
        let expected = merkle_hash(&left, &right, blake3::Hasher::new_keyed(&KEY));
        assert_eq!(root_orig, expected);
    }

    #[test]
    fn test_four_entries() {
        let h1 = make_test_hash(b"entry1");
        let h2 = make_test_hash(b"entry2");
        let h3 = make_test_hash(b"entry3");
        let h4 = make_test_hash(b"entry4");

        // Original implementation
        let root_orig = calc_merkle_root([h1, h2, h3, h4].into_iter());

        // Streaming implementation
        let mut builder = StreamingMerkleBuilder::new();
        builder.add_leaf(h1);
        builder.add_leaf(h2);
        builder.add_leaf(h3);
        builder.add_leaf(h4);
        let root_stream = builder.finalize();

        assert_eq!(root_orig, root_stream, "Four entries: original vs streaming");

        // Verify tree structure
        let left = merkle_hash(&h1, &h2, blake3::Hasher::new_keyed(&KEY));
        let right = merkle_hash(&h3, &h4, blake3::Hasher::new_keyed(&KEY));
        let expected = merkle_hash(&left, &right, blake3::Hasher::new_keyed(&KEY));
        assert_eq!(root_orig, expected);
    }

    #[test]
    fn test_five_entries() {
        let hashes: Vec<[u32; 8]> = (0..5).map(|i| make_test_hash(&[i])).collect();

        // Original implementation
        let root_orig = calc_merkle_root(hashes.iter().copied());

        // Streaming implementation
        let mut builder = StreamingMerkleBuilder::new();
        for hash in &hashes {
            builder.add_leaf(*hash);
        }
        let root_stream = builder.finalize();

        assert_eq!(root_orig, root_stream, "Five entries: original vs streaming");
    }

    #[test]
    fn test_power_of_two_entries() {
        for power in 1..=8 {
            let count = 1usize << power;
            let hashes: Vec<[u32; 8]> = (0..count).map(|i| make_test_hash(&i.to_le_bytes())).collect();

            // Original implementation
            let root_orig = calc_merkle_root(hashes.iter().copied());

            // Streaming implementation
            let mut builder = StreamingMerkleBuilder::new();
            for hash in &hashes {
                builder.add_leaf(*hash);
            }
            let root_stream = builder.finalize();

            assert_eq!(root_orig, root_stream, "Power of 2 ({} entries): original vs streaming", count);
        }
    }

    #[test]
    fn test_non_power_of_two_entries() {
        let test_cases = [3i64, 5, 6, 7, 9, 10, 11, 13, 15, 17, 31, 33, 63, 65, 100];

        for &count in &test_cases {
            let hashes: Vec<[u32; 8]> = (0..count).map(|i| make_test_hash(&i.to_le_bytes())).collect();

            // Original implementation
            let root_orig = calc_merkle_root(hashes.iter().copied());

            // Streaming implementation
            let mut builder = StreamingMerkleBuilder::new();
            for hash in &hashes {
                builder.add_leaf(*hash);
            }
            let root_stream = builder.finalize();

            assert_eq!(root_orig, root_stream, "Non-power of 2 ({} entries): original vs streaming", count);
        }
    }

    #[test]
    fn test_order_matters() {
        let h1 = make_test_hash(b"h1");
        let h2 = make_test_hash(b"h2");

        let root1 = calc_merkle_root([h1, h2].into_iter());
        let root2 = calc_merkle_root([h2, h1].into_iter());

        assert_ne!(root1, root2, "Order should matter");

        // Test with streaming too
        let mut builder1 = StreamingMerkleBuilder::new();
        builder1.add_leaf(h1);
        builder1.add_leaf(h2);
        let stream_root1 = builder1.finalize();

        let mut builder2 = StreamingMerkleBuilder::new();
        builder2.add_leaf(h2);
        builder2.add_leaf(h1);
        let stream_root2 = builder2.finalize();

        assert_ne!(stream_root1, stream_root2, "Order should matter in streaming");
        assert_eq!(root1, stream_root1);
        assert_eq!(root2, stream_root2);
    }

    #[test]
    fn test_consistency_multiple_calls() {
        let hashes: Vec<[u32; 8]> = (0..7).map(|i| make_test_hash(&[i])).collect();

        let root1 = calc_merkle_root(hashes.iter().copied());
        let root2 = calc_merkle_root(hashes.iter().copied());

        assert_eq!(root1, root2, "Multiple calls should produce same result");
    }

    #[test]
    fn test_seq_commitment_leaf() {
        let tx_id = make_test_hash(b"transaction_id");
        let version = 1u16;

        let leaf = seq_commitment_leaf(&tx_id, version);

        // Verify it's deterministic
        let leaf2 = seq_commitment_leaf(&tx_id, version);
        assert_eq!(leaf, leaf2);

        // Verify different versions produce different leaves
        let leaf_v2 = seq_commitment_leaf(&tx_id, 2);
        assert_ne!(leaf, leaf_v2);

        // Verify different tx_ids produce different leaves
        let tx_id2 = make_test_hash(b"different_tx");
        let leaf_diff = seq_commitment_leaf(&tx_id2, version);
        assert_ne!(leaf, leaf_diff);
    }

    #[test]
    fn test_calc_accepted_id_merkle_root() {
        let parent_root = make_test_hash(b"parent_root");
        let accepted_root = make_test_hash(b"accepted_root");

        let result = calc_accepted_id_merkle_root(&parent_root, &accepted_root);

        // Verify it's deterministic
        let result2 = calc_accepted_id_merkle_root(&parent_root, &accepted_root);
        assert_eq!(result, result2);

        // Verify it's the merkle hash
        let expected = merkle_hash(&parent_root, &accepted_root, blake3::Hasher::new_keyed(&KEY));
        assert_eq!(result, expected);
    }

    // Tests with kaspa-merkle for compatibility (only run when dev-dependency is available)
    mod kaspa_compat_tests {
        use super::*;
        use kaspa_hashes::SeqCommitmentMerkleBranchHash;

        #[test]
        fn test_kaspa_merkle_empty() {
            // Our implementation
            let our_root = calc_merkle_root(core::iter::empty());

            // Kaspa implementation
            let kaspa_root = kaspa_merkle::calc_merkle_root_with_hasher::<SeqCommitmentMerkleBranchHash, true>(core::iter::empty());

            assert_eq!(to_kaspa_hash(&our_root), kaspa_root, "Empty tree should match kaspa-merkle");
        }

        #[test]
        fn test_kaspa_merkle_single() {
            let h1 = make_test_hash(b"entry1");

            // Our implementation
            let our_root = calc_merkle_root(core::iter::once(h1));

            // Kaspa implementation
            let kaspa_root = kaspa_merkle::calc_merkle_root_with_hasher::<SeqCommitmentMerkleBranchHash, true>(core::iter::once(
                to_kaspa_hash(&h1),
            ));

            assert_eq!(to_kaspa_hash(&our_root), kaspa_root, "Single entry should match kaspa-merkle");
        }

        #[test]
        fn test_kaspa_merkle_multiple() {
            let hashes: Vec<[u32; 8]> = (0..10).map(|i| make_test_hash(&[i])).collect();

            // Our original implementation
            let our_root = calc_merkle_root(hashes.iter().copied());

            // Our streaming implementation
            let mut builder = StreamingMerkleBuilder::new();
            for hash in &hashes {
                builder.add_leaf(*hash);
            }
            let stream_root = builder.finalize();

            // Kaspa implementation
            let kaspa_hashes: Vec<_> = hashes.iter().map(to_kaspa_hash).collect();
            let kaspa_root =
                kaspa_merkle::calc_merkle_root_with_hasher::<SeqCommitmentMerkleBranchHash, true>(kaspa_hashes.into_iter());

            assert_eq!(our_root, stream_root, "Original vs streaming should match");
            assert_eq!(to_kaspa_hash(&our_root), kaspa_root, "Our implementation should match kaspa-merkle");
        }

        #[test]
        fn test_kaspa_merkle_various_sizes() {
            for count in [0i32, 1, 2, 3, 4, 5, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100] {
                let hashes: Vec<[u32; 8]> = (0..count).map(|i| make_test_hash(&i.to_le_bytes())).collect();

                // Our original implementation
                let our_root = calc_merkle_root(hashes.iter().copied());

                // Our streaming implementation
                let mut builder = StreamingMerkleBuilder::new();
                for hash in &hashes {
                    builder.add_leaf(*hash);
                }
                let stream_root = builder.finalize();

                // Kaspa implementation
                let kaspa_hashes: Vec<_> = hashes.iter().map(to_kaspa_hash).collect();
                let kaspa_root =
                    kaspa_merkle::calc_merkle_root_with_hasher::<SeqCommitmentMerkleBranchHash, true>(kaspa_hashes.into_iter());

                assert_eq!(our_root, stream_root, "Size {}: original vs streaming", count);
                assert_eq!(to_kaspa_hash(&our_root), kaspa_root, "Size {}: should match kaspa-merkle", count);
            }
        }

        #[test]
        fn test_full_workflow_compatibility() {
            for block_num in 0..10 {
                let tx_count = 300 + block_num;
                let tx_hashes: Vec<[u32; 8]> = (0..tx_count).map(|i| make_test_hash(&[block_num as u8, i as u8])).collect();

                // Our implementation - original
                let our_block_root = calc_merkle_root(tx_hashes.iter().copied());

                // Our implementation - streaming
                let mut builder = StreamingMerkleBuilder::new();
                for hash in &tx_hashes {
                    builder.add_leaf(*hash);
                }
                let stream_block_root = builder.finalize();

                // Kaspa implementation
                let kaspa_hashes: Vec<_> = tx_hashes.iter().map(to_kaspa_hash).collect();
                let kaspa_block_root =
                    kaspa_merkle::calc_merkle_root_with_hasher::<SeqCommitmentMerkleBranchHash, true>(kaspa_hashes.into_iter());

                assert_eq!(our_block_root, stream_block_root, "Block {}: original vs streaming", block_num);
                assert_eq!(to_kaspa_hash(&our_block_root), kaspa_block_root, "Block {}: should match kaspa", block_num);

                // Update commitments
                let our_commitment = calc_accepted_id_merkle_root(&ZERO_HASH, &our_block_root);
                let kaspa_commitment = kaspa_merkle::merkle_hash_with_hasher(
                    kaspa_hashes::ZERO_HASH,
                    kaspa_block_root,
                    SeqCommitmentMerkleBranchHash::new(),
                );

                assert_eq!(to_kaspa_hash(&our_commitment), kaspa_commitment, "Block {}: commitment chain should match", block_num);
            }
        }
    }
}
