use super::HasherExtensions;
use crate::header::Header;
use kaspa_hashes::{Hash, HasherBase};

/// Returns the header hash using the provided nonce+timestamp instead of those in the header.
#[inline]
pub fn hash_override_nonce_time(header: &Header, nonce: u64, timestamp: u64) -> Hash {
    let mut hasher = kaspa_hashes::BlockHash::new();
    hasher.update(header.version.to_le_bytes()).write_len(header.parents_by_level.len()); // Write the number of parent levels

    // Write parents at each level
    header.parents_by_level.iter().for_each(|level| {
        hasher.write_var_array(level);
    });

    // Write all header fields
    hasher
        .update(header.hash_merkle_root)
        .update(header.accepted_id_merkle_root)
        .update(header.utxo_commitment)
        .update(timestamp.to_le_bytes())
        .update(header.bits.to_le_bytes())
        .update(nonce.to_le_bytes())
        .update(header.daa_score.to_le_bytes())
        .update(header.blue_score.to_le_bytes())
        .write_blue_work(header.blue_work)
        .update(header.pruning_point);

    hasher.finalize()
}

/// Returns the header hash.
pub fn hash(header: &Header) -> Hash {
    hash_override_nonce_time(header, header.nonce, header.timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{blockhash, BlueWorkType};

    #[test]
    fn test_header_hashing() {
        let header = Header::new_finalized(
            1,
            vec![vec![1.into()]],
            Default::default(),
            Default::default(),
            Default::default(),
            234,
            23,
            567,
            0,
            0.into(),
            0,
            Default::default(),
        );
        assert_ne!(blockhash::NONE, header.hash);
    }

    #[test]
    fn test_hash_blue_work() {
        let tests: Vec<(BlueWorkType, Vec<u8>)> =
            vec![(0.into(), vec![0, 0, 0, 0, 0, 0, 0, 0]), (123456.into(), vec![3, 0, 0, 0, 0, 0, 0, 0, 1, 226, 64])];

        for test in tests {
            let mut hasher = kaspa_hashes::BlockHash::new();
            hasher.write_blue_work(test.0);

            let mut hasher2 = kaspa_hashes::BlockHash::new();
            hasher2.update(test.1);
            assert_eq!(hasher.finalize(), hasher2.finalize())
        }
    }
}
