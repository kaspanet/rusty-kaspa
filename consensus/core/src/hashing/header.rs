use super::HasherExtensions;
use crate::header::Header;
use hashes::{Hash, HasherBase};

/// Returns the header hash.
pub fn hash(header: &Header) -> Hash {
    let mut hasher = hashes::BlockHash::new();
    hasher
        .update(header.version.to_le_bytes())
        .write_len(header.parents_by_level.len()); // Write the number of parent levels

    // Write parents at each level
    for parents_at_level in header.parents_by_level.iter() {
        hasher.write_var_array(parents_at_level);
    }

    // Write all header fields
    hasher
        .update(header.hash_merkle_root)
        .update(header.accepted_id_merkle_root)
        .update(header.utxo_commitment)
        .update(header.timestamp.to_le_bytes())
        .update(header.bits.to_le_bytes())
        .update(header.nonce.to_le_bytes())
        .update(header.daa_score.to_le_bytes())
        .update(header.blue_score.to_le_bytes())
        .write_blue_work(header.blue_work)
        .update(header.pruning_point);

    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{blockhash, BlueWorkType};

    #[test]
    fn test_header_hashing() {
        let header = Header::new(1, vec![1.into()], 234, 23, 567, 0, 0, 0);
        assert_ne!(blockhash::NONE, header.hash);

        // TODO: tests comparing to golang ref
    }

    #[test]
    fn test_hash_blue_work() {
        let tests: Vec<(BlueWorkType, Vec<u8>)> =
            vec![(0, vec![0, 0, 0, 0, 0, 0, 0, 0]), (123456, vec![3, 0, 0, 0, 0, 0, 0, 0, 1, 226, 64])];

        for test in tests {
            let mut hasher = hashes::BlockHash::new();
            hasher.write_blue_work(test.0);

            let mut hasher2 = hashes::BlockHash::new();
            hasher2.update(test.1);
            assert_eq!(hasher.finalize(), hasher2.finalize())
        }
    }
}
