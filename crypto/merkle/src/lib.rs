use kaspa_hashes::{Hash, HasherBase, MerkleBranchHash, ZERO_HASH};

pub fn calc_merkle_root(hashes: impl ExactSizeIterator<Item = Hash>) -> Hash {
    if hashes.len() == 0 {
        return ZERO_HASH;
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
            merkles[offset] = Some(merkle_hash(merkles[i].unwrap(), merkles[i + 1].unwrap_or(ZERO_HASH)));
        }
        offset += 1
    }
    merkles.last().unwrap().unwrap()
}

fn merkle_hash(left: Hash, right: Hash) -> Hash {
    let mut hasher = MerkleBranchHash::new();
    hasher.update(left).update(right);
    hasher.finalize()
}
