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

    type Count = u32;

    pub fn serialize<S>(parents: &[Vec<Hash>], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            return <&[Vec<Hash>] as serde::Serialize>::serialize(&parents, serializer);
        }

        struct Run<'a> {
            cumulative: Count,
            vec: &'a Vec<Hash>,
        }

        if parents.is_empty() {
            let seq = serializer.serialize_seq(Some(0))?;
            return seq.end();
        }

        let mut runs: Vec<Run> = Vec::new();
        let mut cumulative: Count = 0;
        let mut current_vec = &parents[0];
        let mut current_len: Count = 1;

        for vec in &parents[1..] {
            if vec == current_vec {
                current_len = current_len.checked_add(1).ok_or_else(|| S::Error::custom("run length overflow"))?;
            } else {
                cumulative = cumulative.checked_add(current_len).ok_or_else(|| S::Error::custom("cumulative length overflow"))?;
                runs.push(Run { cumulative, vec: current_vec });

                current_vec = vec;
                current_len = 1;
            }
        }

        let mut seq = serializer.serialize_seq(Some(runs.len()))?;
        for run in &runs {
            let elem = (run.cumulative, run.vec);
            seq.serialize_element(&elem)?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Vec<Hash>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            return <Vec<Vec<Hash>>>::deserialize(deserializer);
        }

        type Pair = (Count, Vec<Hash>);
        let pairs: Vec<Pair> = <Vec<Pair> as Deserialize>::deserialize(deserializer)?;

        let mut out: Vec<Vec<Hash>> = Vec::new();
        let mut prev_cum: Count = 0usize as Count;
        let mut last_vec: Option<&[Hash]> = None;

        for (cum, v) in pairs.iter() {
            if *cum < prev_cum {
                return Err(D::Error::custom("non-monotonic cumulative count in VRLE stream"));
            }
            let run_len = (*cum)
                .checked_sub(prev_cum)
                .ok_or_else(|| D::Error::custom("invalid cumulative count (underflow)"))?;

            if run_len == 0 {
                return Err(D::Error::custom("zero-length run in VRLE stream"));
            }
            if let Some(prev) = last_vec {
                if prev == v.as_slice() {
                    return Err(D::Error::custom(
                        "adjacent runs contain identical vectors (should be merged)",
                    ));
                }
            }

            for _ in 0..run_len {
                out.push(v.clone());
            }

            prev_cum = *cum;
            last_vec = Some(v.as_slice());
        }

        Ok(out)
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
