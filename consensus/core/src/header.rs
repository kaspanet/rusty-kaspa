use crate::{hashing, BlueWorkType};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash;
use kaspa_utils::{iter::IterExtensions, mem_size::MemSizeEstimator};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::{mem::size_of, slice};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompressedParents(Vec<(u8, Vec<Hash>)>);

impl From<Vec<Vec<Hash>>> for CompressedParents {
    fn from(parents: Vec<Vec<Hash>>) -> Self {
        Self::from_vec(parents)
    }
}

impl CompressedParents {
    fn compress(parents: Vec<Vec<Hash>>) -> Result<Vec<(u8, Vec<Hash>)>, &'static str> {
        if parents.len() > u8::MAX as usize {
            return Err("Parents by level exceeds maximum levels of 255");
        }

        if parents.is_empty() {
            return Ok(Vec::new());
        }

        // Casting count from usize to u8 is safe because of the check above
        Ok(parents.into_iter().dedup_with_cumulative_count().map(|(count, level)| (count as u8, level)).collect())
    }

    pub fn try_from_vec(parents: Vec<Vec<Hash>>) -> Result<Self, &'static str> {
        Self::compress(parents).map(Self)
    }

    pub fn from_vec(parents: Vec<Vec<Hash>>) -> Self {
        Self::try_from_vec(parents).expect("Parents by level exceeds maximum levels of 255")
    }
    fn from_cumulative_runs(runs: Vec<(u8, Vec<Hash>)>) -> Self {
        if runs.is_empty() {
            return Self(Vec::new());
        }

        let mut prev = 0u8;
        let mut storage = Vec::with_capacity(runs.len());

        for (cum, level_parents) in runs {
            assert!(cum > prev, "non-monotonic cumulative parents_by_level");
            storage.push((cum, level_parents));
            prev = cum;
        }

        Self(storage)
    }

    pub fn to_vec(&self) -> Vec<Vec<Hash>> {
        let mut out = Vec::new();
        let mut prev = 0u8;

        for (cum, level_parents) in &self.0 {
            let run_len = cum - prev;
            for _ in 0..run_len {
                out.push(level_parents.clone());
            }
            prev = *cum;
        }

        out
    }

    pub fn len(&self) -> usize {
        self.0.last().map(|(cum, _)| *cum as usize).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Vec<Hash>> {
        if index >= self.len() {
            return None;
        }
        let target = index as u8;
        let i = self.0.binary_search_by_key(&target, |(cum, _)| *cum - 1).unwrap_or_else(|i| i);
        Some(&self.0[i].1)
    }

    pub fn runs(&self) -> &[(u8, Vec<Hash>)] {
        &self.0
    }

    pub fn iter(&self) -> CompressedParentsIter<'_> {
        CompressedParentsIter { runs: self.0.iter(), remaining_in_run: 0, current_vec: None, prev_cumulative: 0 }
    }
}

pub struct CompressedParentsIter<'a> {
    runs: slice::Iter<'a, (u8, Vec<Hash>)>,
    remaining_in_run: u8,
    current_vec: Option<&'a Vec<Hash>>,
    prev_cumulative: u8,
}

impl<'a> Iterator for CompressedParentsIter<'a> {
    type Item = &'a Vec<Hash>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_in_run == 0 {
            let (cum, vec) = self.runs.next()?;
            let run_len = cum - self.prev_cumulative;
            self.prev_cumulative = *cum;
            self.remaining_in_run = run_len;
            self.current_vec = Some(vec);
        }

        debug_assert!(self.remaining_in_run > 0);
        self.remaining_in_run -= 1;
        self.current_vec
    }
}

impl<'a> IntoIterator for &'a CompressedParents {
    type Item = &'a Vec<Hash>;
    type IntoIter = CompressedParentsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl From<CompressedParents> for Vec<Vec<Hash>> {
    fn from(value: CompressedParents) -> Self {
        value.to_vec()
    }
}

impl From<&CompressedParents> for Vec<Vec<Hash>> {
    fn from(value: &CompressedParents) -> Self {
        value.to_vec()
    }
}

impl std::ops::Index<usize> for CompressedParents {
    type Output = Vec<Hash>;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

impl Serialize for CompressedParents {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            return serde::Serialize::serialize(&self.to_vec(), serializer);
        }

        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for (cum, vec) in &self.0 {
            seq.serialize_element(&(cum, vec))?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for CompressedParents {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let parents: Vec<Vec<Hash>> = <Vec<Vec<Hash>> as Deserialize>::deserialize(deserializer)?;
            return Ok(Self::from_vec(parents));
        }

        let runs: Vec<(u8, Vec<Hash>)> = <Vec<(u8, Vec<Hash>)> as Deserialize>::deserialize(deserializer)?;
        Ok(Self::from_cumulative_runs(runs))
    }
}

impl BorshSerialize for CompressedParents {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.to_vec(), writer)
    }
}

impl BorshDeserialize for CompressedParents {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let parents = Vec::<Vec<Hash>>::deserialize_reader(reader)?;
        Self::try_from_vec(parents).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl MemSizeEstimator for CompressedParents {
    fn estimate_mem_bytes(&self) -> usize {
        let runs_overhead = self.0.capacity() * size_of::<(u8, Vec<Hash>)>();
        let vectors_bytes: usize = self.0.iter().map(|(_, vec)| vec.capacity() * size_of::<Hash>()).sum();
        size_of::<Self>() + runs_overhead + vectors_bytes
    }
}

/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    /// Cached hash
    pub hash: Hash,
    pub version: u16,
    pub parents_by_level: CompressedParents,
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
            parents_by_level: CompressedParents::from_vec(parents_by_level),
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
        self.parents_by_level.get(0).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn parents_by_level_vec(&self) -> Vec<Vec<Hash>> {
        Vec::from(&self.parents_by_level)
    }

    pub fn set_parents_by_level_vec(&mut self, parents: Vec<Vec<Hash>>) {
        self.parents_by_level = parents.into();
    }

    /// WARNING: To be used for test purposes only
    pub fn from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Header {
        Header {
            version: crate::constants::BLOCK_VERSION,
            hash,
            parents_by_level: CompressedParents::from_vec(vec![parents]),
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
        size_of::<Self>() - size_of::<CompressedParents>() + self.parents_by_level.estimate_mem_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_math::Uint192;
    use serde_json::Value;

    fn hash(val: u8) -> Hash {
        Hash::from(val as u64)
    }

    fn vec_from(slice: &[u8]) -> Vec<Hash> {
        slice.iter().map(|&v| hash(v)).collect()
    }

    fn serialize_parents(parents: &[Vec<Hash>]) -> Vec<u8> {
        let compressed = CompressedParents::from_vec(parents.to_vec());
        bincode::serialize(&compressed).unwrap()
    }

    fn deserialize_parents(bytes: &[u8]) -> bincode::Result<Vec<Vec<Hash>>> {
        let parents: CompressedParents = bincode::deserialize(bytes)?;
        Ok(parents.to_vec())
    }

    fn json_value(parents: &[Vec<Hash>]) -> serde_json::Value {
        #[derive(Serialize)]
        struct Wrapper {
            parents: CompressedParents,
        }

        serde_json::to_value(Wrapper { parents: CompressedParents::from_vec(parents.to_vec()) }).unwrap()
    }

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

    #[test]
    fn parents_vrle_round_trip_multiple_runs() {
        let parents = vec![
            vec_from(&[1, 2, 3]),
            vec_from(&[1, 2, 3]),
            vec_from(&[1, 2, 3]),
            vec_from(&[4, 5]),
            vec_from(&[4, 5]),
            vec_from(&[6]),
        ];

        let bytes = serialize_parents(&parents);
        let decoded = deserialize_parents(&bytes).unwrap();
        assert_eq!(decoded, parents);
    }

    #[test]
    fn parents_vrle_round_trip_single_run() {
        let repeated = vec_from(&[9, 8, 7]);
        let parents = vec![repeated.clone(), repeated.clone(), repeated.clone()];

        let bytes = serialize_parents(&parents);
        let decoded = deserialize_parents(&bytes).unwrap();
        assert_eq!(decoded, parents);
    }

    #[test]
    fn parents_vrle_round_trip_empty() {
        let bytes = serialize_parents(&[]);
        let decoded = deserialize_parents(&bytes).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn parents_vrle_preserves_human_readable_json() {
        let parents = vec![vec_from(&[1, 2]), vec_from(&[3, 4])];

        let json = json_value(&parents);
        let expected = serde_json::json!({ "parents": parents });
        assert_eq!(json, expected);

        #[derive(Deserialize)]
        struct Wrapper {
            parents: CompressedParents,
        }

        let decoded: Wrapper = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.parents.to_vec(), parents);
    }

    #[test]
    fn compressed_parents_len_and_get() {
        let first = vec_from(&[1]);
        let second = vec_from(&[2, 3]);
        let third = vec_from(&[4]);
        let parents = vec![first.clone(), first.clone(), second.clone(), second.clone(), third.clone()];
        let compressed = CompressedParents::from_vec(parents.clone());

        assert_eq!(compressed.len(), parents.len());
        assert_eq!(compressed.get(0), Some(&first));
        assert_eq!(compressed.get(1), Some(&first));
        assert_eq!(compressed.get(2), Some(&second));
        assert_eq!(compressed.get(10), None);

        let collected: Vec<&Vec<Hash>> = compressed.iter().collect();
        let expected = vec![&first, &first, &second, &second, &third];
        assert_eq!(collected, expected);
    }

    #[test]
    fn compressed_parents_binary_format_matches_runs() {
        let parents = vec![vec_from(&[1, 2, 3]), vec_from(&[1, 2, 3]), vec_from(&[4])];
        let compressed = CompressedParents::from_vec(parents);

        let encoded = bincode::serialize(&compressed).unwrap();
        let expected = bincode::serialize(compressed.runs()).unwrap();
        assert_eq!(encoded, expected);
    }
}
