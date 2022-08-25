use crate::hashing::HasherExtensions;
use hashes::{BlockHash, Hash, Hasher};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Header {
    pub hash: Hash, // cached hash
    pub version: u16,
    pub parents_by_level: Vec<Vec<Hash>>,
    pub timestamp: u64, // timestamp is in millis
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    // TODO: add parent levels and all remaining fields
}

impl Header {
    pub fn new(version: u16, parents: Vec<Hash>, timestamp: u64, bits: u32, nonce: u64, daa_score: u64) -> Self {
        let mut hasher = BlockHash::new();
        hasher
            .update(version.to_le_bytes())
            .write_var_array(&parents) // TODO: hash multi-level parents
            .update(timestamp.to_le_bytes())
            .update(bits.to_le_bytes())
            .update(nonce.to_le_bytes())
            .update(daa_score.to_le_bytes());

        Self {
            hash: hasher.finalize(),
            version,
            parents_by_level: vec![parents], // TODO: Handle multi level parents properly
            nonce,
            timestamp,
            daa_score,
            bits,
        }
    }

    pub fn direct_parents(&self) -> &Vec<Hash> {
        &self.parents_by_level[0]
    }
}
