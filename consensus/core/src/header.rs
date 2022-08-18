use hashes::{BlockHash, Hash, Hasher};

pub struct Header {
    pub hash: Hash, // cached hash
    pub version: u16,
    pub parents: Vec<Hash>,
    pub nonce: u64,
    pub timestamp: u64, // timestamp is in millis
                        // TODO: add parent levels and all remaining fields
}

impl Header {
    pub fn new(version: u16, parents: Vec<Hash>, nonce: u64, timestamp: u64) -> Self {
        let mut hasher = BlockHash::new();
        hasher
            .update(timestamp.to_le_bytes())
            .update(version.to_le_bytes())
            .update((parents.len() as u64).to_le_bytes());

        for parent in parents.iter() {
            // TODO: Hash properly for multi-level parents
            hasher.write(parent);
        }
        hasher.update(nonce.to_le_bytes());
        Self { hash: hasher.finalize(), version, parents, nonce, timestamp }
    }
}
