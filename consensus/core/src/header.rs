use hashes::{BlockHash, Hash, Hasher};

pub struct Header {
    pub hash: Hash, // cached hash
    pub version: u16,
    pub parents: Vec<Hash>,
    pub nonce: u64,
    // TODO: add parent levels and all remaining fields
}

impl Header {
    pub fn new(version: u16, parents: Vec<Hash>, nonce: u64) -> Self {
        let mut hasher = BlockHash::new();
        hasher
            .update(version.to_le_bytes())
            .update((parents.len() as u64).to_le_bytes());
        for parent in parents.iter() {
            hasher.write(parent);
        }
        hasher.update(nonce.to_le_bytes());
        Self { hash: hasher.finalize(), version, parents, nonce }
    }

    /// Temp function for injecting the hash externally
    pub fn from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Self {
        Self { version: 0, hash, parents, nonce: 0 }
    }
}
