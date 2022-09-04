use crate::{hashing, BlueWorkType};
use hashes::Hash;
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
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    // TODO: add parent levels and all remaining fields
}

impl Header {
    pub fn new(
        version: u16, parents: Vec<Hash>, timestamp: u64, bits: u32, nonce: u64, daa_score: u64,
        blue_work: BlueWorkType, blue_score: u64,
    ) -> Self {
        let mut header = Self {
            hash: Default::default(), // Temp init before the hashing below
            version,
            parents_by_level: vec![parents], // TODO: Handle multi level parents properly
            nonce,
            timestamp,
            daa_score,
            bits,
            blue_work,
            blue_score,
        };
        header.finalize();
        header
    }

    pub fn finalize(&mut self) {
        self.hash = hashing::header::hash(self);
    }

    pub fn direct_parents(&self) -> &Vec<Hash> {
        &self.parents_by_level[0]
    }
}
