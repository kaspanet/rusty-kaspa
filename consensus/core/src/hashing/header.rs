use super::HasherExtensions;
use crate::{header::Header, BlueWorkType};
use hashes::{Hash, Hasher};

/// Returns the header hash.
pub fn header_hash(header: &Header) -> Hash {
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
        .update(header.timestamp.to_le_bytes())
        .update(header.bits.to_le_bytes())
        .update(header.nonce.to_le_bytes())
        .update(header.daa_score.to_le_bytes())
        .update(header.blue_score.to_le_bytes())
        .update(serialize_blue_work(header.blue_work));

    hasher.finalize()
}

fn serialize_blue_work(work: BlueWorkType) -> Vec<u8> {
    let be_bytes = work.to_be_bytes();
    let start = be_bytes
        .iter()
        .cloned()
        .position(|byte| byte != 0);

    if let Some(start) = start {
        be_bytes[start..].to_vec()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockhash;

    #[test]
    fn test_header_hashing() {
        let header = Header::new(1, vec![1.into()], 234, 23, 567, 0, 0, 0);
        assert_ne!(blockhash::NONE, header.hash);

        // TODO: tests comparing to golang ref
    }

    #[test]
    fn test_serialize_blue_work() {
        assert_eq!(serialize_blue_work(123456), vec![1, 226, 64])
    }
}
