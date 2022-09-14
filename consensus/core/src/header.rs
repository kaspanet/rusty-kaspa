use crate::{hashing, BlueWorkType};
use hashes::Hash;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Header {
    pub hash: Hash, // Cached hash
    pub version: u16,
    pub parents_by_level: Vec<Vec<Hash>>,
    pub hash_merkle_root: Hash,
    pub accepted_id_merkle_root: Hash,
    pub utxo_commitment: Hash,
    pub timestamp: u64, // Timestamp is in milliseconds
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: Hash,
}

impl Header {
    pub fn new(
        version: u16,
        parents: Vec<Hash>,
        hash_merkle_root: Hash,
        timestamp: u64,
        bits: u32,
        nonce: u64,
        daa_score: u64,
        blue_work: BlueWorkType,
        blue_score: u64,
    ) -> Self {
        let mut header = Self {
            hash: Default::default(), // Temp init before the finalize below
            version,
            parents_by_level: vec![parents], // TODO: Handle multi level parents properly
            hash_merkle_root,
            accepted_id_merkle_root: Default::default(),
            utxo_commitment: Default::default(),
            nonce,
            timestamp,
            daa_score,
            bits,
            blue_work,
            blue_score,
            pruning_point: Default::default(),
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
