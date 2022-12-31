use crate::{hashing, BlueWorkType};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use hashes::Hash;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: u16,
        parents_by_level: Vec<Vec<Hash>>,
        hash_merkle_root: Hash,
        accepted_id_merkle_root: Hash,
        utxo_commitment: Hash,
        timestamp: u64,
        bits: u32,
        nonce: u64,
        daa_score: u64,
        blue_work: BlueWorkType,
        blue_score: u64,
        pruning_point: Hash,
    ) -> Self {
        let mut header = Self {
            hash: Default::default(), // Temp init before the finalize below
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            nonce,
            timestamp,
            daa_score,
            bits,
            blue_work,
            blue_score,
            pruning_point,
        };
        header.finalize();
        header
    }

    /// Finalizes the header and recomputes the header hash
    pub fn finalize(&mut self) {
        self.hash = hashing::header::hash(self);
    }

    pub fn direct_parents(&self) -> &Vec<Hash> {
        &self.parents_by_level[0]
    }
}
