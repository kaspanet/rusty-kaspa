use kaspa_hashes::{Hash, Hasher, MerkleBranchHash, ZERO_HASH};

pub fn calc_merkle_root(hashes: impl ExactSizeIterator<Item = Hash>) -> Hash {
    calc_merkle_root_with_hasher::<MerkleBranchHash, false>(hashes)
}

pub fn merkle_hash(left: Hash, right: Hash) -> Hash {
    merkle_hash_with_hasher(left, right, MerkleBranchHash::new())
}

pub fn merkle_hash_with_hasher(left: Hash, right: Hash, mut hasher: impl Hasher) -> Hash {
    hasher.update(left).update(right);
    hasher.finalize()
}

pub fn calc_merkle_root_with_hasher<H: Hasher, const USE_BRANCH_HASHER_FOR_SINGLE: bool>(
    mut hashes: impl ExactSizeIterator<Item = Hash>,
) -> Hash {
    match hashes.len() {
        0 => return cold_path_empty(),
        1 if USE_BRANCH_HASHER_FOR_SINGLE => return merkle_hash_with_hasher(hashes.next().unwrap(), ZERO_HASH, H::default()),
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
            merkles[offset] = Some(merkle_hash_with_hasher(merkles[i].unwrap(), merkles[i + 1].unwrap_or(ZERO_HASH), H::default()));
        }
        offset += 1
    }
    merkles.last().unwrap().unwrap()
}

#[inline(never)]
#[cold]
fn cold_path_empty() -> Hash {
    ZERO_HASH
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::{HasherBase, SeqCommitmentMerkleBranchHash, TransactionHash};
    use std::iter;
    fn seq_comm_merkle_root(hashes: impl ExactSizeIterator<Item = Hash>) -> Hash {
        calc_merkle_root_with_hasher::<SeqCommitmentMerkleBranchHash, true>(hashes)
    }
    fn make_hash(data: &[u8]) -> Hash {
        let mut hasher = TransactionHash::new();
        hasher.update(data);
        hasher.finalize()
    }
    #[test]
    fn test_empty_returns_zero_hash() {
        let root = calc_merkle_root(std::iter::empty());
        assert_eq!(root, ZERO_HASH, "Empty input should return ZERO_HASH");

        let seq_root = seq_comm_merkle_root(std::iter::empty());
        assert_eq!(seq_root, ZERO_HASH, "Empty input should return ZERO_HASH for seq_comm");
    }

    #[test]
    fn test_single_entry_returns_hash() {
        let entry = make_hash(b"single_entry");
        let root = calc_merkle_root(iter::once(entry));
        let expected = entry;
        assert_eq!(root, expected);

        let expected = merkle_hash_with_hasher(entry, ZERO_HASH, SeqCommitmentMerkleBranchHash::default());
        let seq_comm_root = seq_comm_merkle_root(iter::once(entry));
        assert_eq!(seq_comm_root, expected, "Single entry should return merkle hash of entry with ZERO_HASH");
    }

    #[test]
    fn test_two_entries_returns_hash_of_both() {
        let h1 = make_hash(b"entry1");
        let h2 = make_hash(b"entry2");

        let root = calc_merkle_root([h1, h2].into_iter());
        let expected = merkle_hash(h1, h2);
        assert_eq!(root, expected, "Two entries should hash directly together");

        let seq_root = seq_comm_merkle_root([h1, h2].into_iter());
        let seq_expected = merkle_hash_with_hasher(h1, h2, SeqCommitmentMerkleBranchHash::default());
        assert_eq!(seq_root, seq_expected, "Two entries should hash directly together for seq_comm");
    }

    #[test]
    fn test_three_entries() {
        // Tree structure for 3 entries (next_pot = 4):
        // Indices: [h1, h2, h3, None, ..., result]
        // Level 0: h1, h2, h3, None
        // Level 1: hash(h1,h2), hash(h3,ZERO)
        // Level 2: hash(hash(h1,h2), hash(h3,ZERO))
        let h1 = make_hash(b"h1");
        let h2 = make_hash(b"h2");
        let h3 = make_hash(b"h3");

        let root = calc_merkle_root([h1, h2, h3].into_iter());

        let left = merkle_hash(h1, h2);
        let right = merkle_hash(h3, ZERO_HASH);
        let expected = merkle_hash(left, right);

        assert_eq!(root, expected, "Three entries should build correct tree");
    }

    #[test]
    fn test_four_entries() {
        // Tree structure for 4 entries (next_pot = 4):
        // Level 0: h1, h2, h3, h4
        // Level 1: hash(h1,h2), hash(h3,h4)
        // Level 2: hash(hash(h1,h2), hash(h3,h4))
        let h1 = make_hash(b"h1");
        let h2 = make_hash(b"h2");
        let h3 = make_hash(b"h3");
        let h4 = make_hash(b"h4");

        let root = calc_merkle_root([h1, h2, h3, h4].into_iter());

        let left = merkle_hash(h1, h2);
        let right = merkle_hash(h3, h4);
        let expected = merkle_hash(left, right);

        assert_eq!(root, expected, "Four entries should build correct balanced tree");
    }
    #[test]
    fn test_consistency_multiple_calls() {
        let hashes: Vec<Hash> = (0..5).map(|i| make_hash(&[i])).collect();

        let root1 = calc_merkle_root(hashes.clone().into_iter());
        let root2 = calc_merkle_root(hashes.clone().into_iter());

        assert_eq!(root1, root2, "Multiple calls with same input should produce same result");
    }

    #[test]
    fn test_order_matters() {
        let h1 = make_hash(b"h1");
        let h2 = make_hash(b"h2");

        let root1 = calc_merkle_root([h1, h2].into_iter());
        let root2 = calc_merkle_root([h2, h1].into_iter());

        assert_ne!(root1, root2, "Order of hashes should matter");
    }
}
