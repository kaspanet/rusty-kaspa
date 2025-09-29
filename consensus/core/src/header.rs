use crate::{hashing, BlueWorkType};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};

/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    /// Cached hash
    pub hash: Hash,
    pub version: u16,
    #[serde(with = "parents_by_level_format")]
    pub parents_by_level: Vec<Vec<Hash>>,
    pub hash_merkle_root: Hash,
    pub accepted_id_merkle_root: Hash,
    pub utxo_commitment: Hash,
    /// Timestamp is in milliseconds
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: Hash,
}

impl Header {
    #[allow(clippy::too_many_arguments)]
    pub fn new_finalized(
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

    pub fn direct_parents(&self) -> &[Hash] {
        if self.parents_by_level.is_empty() {
            &[]
        } else {
            &self.parents_by_level[0]
        }
    }

    /// WARNING: To be used for test purposes only
    pub fn from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Header {
        Header {
            version: crate::constants::BLOCK_VERSION,
            hash,
            parents_by_level: vec![parents],
            hash_merkle_root: Default::default(),
            accepted_id_merkle_root: Default::default(),
            utxo_commitment: Default::default(),
            nonce: 0,
            timestamp: 0,
            daa_score: 0,
            bits: 0,
            blue_work: 0.into(),
            blue_score: 0,
            pruning_point: Default::default(),
        }
    }
}

impl AsRef<Header> for Header {
    fn as_ref(&self) -> &Header {
        self
    }
}

impl MemSizeEstimator for Header {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>() + self.parents_by_level.iter().map(|l| l.len()).sum::<usize>() * size_of::<Hash>()
    }
}

pub mod parents_by_level_format {
    use kaspa_hashes::Hash;
    use serde::{
        self,
        de::Error,
        ser::{Error as SerErr, SerializeSeq},
        Deserialize, Deserializer, Serializer,
    };

    /// ## Serializer
    ///
    /// Serializes `Vec<Vec<Hash>>` into a run-length encoded (RLE) sequence of `(u8, Vec<Hash>)`.
    /// The `u8` represents the cumulative count of inner vectors at the end of a run.
    ///
    /// For example: `[[A], [A], [B]]` becomes `[(2, [A]), (3, [B])]`.
    pub fn serialize<S>(parents: &[Vec<Hash>], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        /*

        from:

        [
            [A, B, C],
            [A, B, C],
            [A, B, C],
            [A, C],
            [A, C],
            [Z]
        ]

        to:

        [
            (3, [A, B, C]),
            (5, [A, B]),
            (6, [Z])
        ]
        */

        /*
        Option 1 (not to do):
            1. convert the &[Vec<Hash>] to Vec<(u8, Vec<Hash>)>
            2. serialize the result normally

        Option 2:
            Use the serialize_seq function
        */

        // 1. find count

        let mut seq = serializer.serialize_seq(Some(count))?;

        // 2. loop: 
        seq.serialize_element(...)

        // 3. end
        seq.end()
        // todo!()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Vec<Hash>>, D::Error>
    where
        D: Deserializer<'de>,
    {

        // if deserializer.is_human_readable() returns true then ser/deser normally 

        // simply deser into Vec<(u8, Vec<Hash>)> and then convert to Vec<Vec<Hash>>
        todo!()

        
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_math::Uint192;
    use serde_json::Value;

    #[test]
    fn test_header_ser() {
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
            Uint192([0x1234567890abcfed, 0xc0dec0ffeec0ffee, 0x1234567890abcdef]),
            u64::MAX,
            Default::default(),
        );
        let json = serde_json::to_string(&header).unwrap();
        println!("{}", json);

        let v = serde_json::from_str::<Value>(&json).unwrap();
        let blue_work = v.get("blueWork").expect("missing `blueWork` property");
        let blue_work = blue_work.as_str().expect("`blueWork` is not a string");
        assert_eq!(blue_work, "1234567890abcdefc0dec0ffeec0ffee1234567890abcfed");
        let blue_score = v.get("blueScore").expect("missing `blueScore` property");
        let blue_score: u64 = blue_score.as_u64().expect("blueScore is not a u64 compatible value");
        assert_eq!(blue_score, u64::MAX);

        let h = serde_json::from_str::<Header>(&json).unwrap();
        assert!(h.blue_score == header.blue_score && h.blue_work == header.blue_work);
    }
}
