use crate::{hashing, BlueWorkType};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use js_sys::{Array, Object};
use kaspa_hashes::Hash;
use kaspa_utils::hex::ToHex;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::*;
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct Header {
    #[wasm_bindgen(skip)]
    pub hash: Hash, // Cached hash
    pub version: u16,
    #[wasm_bindgen(skip)]
    pub parents_by_level: Vec<Vec<Hash>>,
    #[wasm_bindgen(skip)]
    pub hash_merkle_root: Hash,
    #[wasm_bindgen(skip)]
    pub accepted_id_merkle_root: Hash,
    #[wasm_bindgen(skip)]
    pub utxo_commitment: Hash,
    pub timestamp: u64, // Timestamp is in milliseconds
    pub bits: u32,
    pub nonce: u64,
    #[wasm_bindgen(js_name = "daaScore")]
    pub daa_score: u64,
    #[wasm_bindgen(skip)]
    pub blue_work: BlueWorkType,
    #[wasm_bindgen(js_name = "blueScore")]
    pub blue_score: u64,
    #[wasm_bindgen(skip)]
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

#[wasm_bindgen]
impl Header {
    /// Finalizes the header and recomputes (updates) the header hash
    /// @return { String } header hash
    #[wasm_bindgen(js_name = finalize)]
    pub fn finalize_js(&mut self) -> String {
        self.hash = hashing::header::hash(self);
        self.hash.to_hex()
    }

    #[wasm_bindgen(constructor)]
    pub fn constructor(js_value: JsValue) -> std::result::Result<Header, JsError> {
        Ok(js_value.try_into()?)
    }

    /// Obtain `JSON` representation of the header. JSON representation
    /// should be obtained using WASM, to ensure proper serialization of
    /// big integers.
    #[wasm_bindgen(js_name = asJSON)]
    pub fn as_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    #[wasm_bindgen(getter = hash)]
    pub fn get_hash_as_hex(&self) -> String {
        self.hash.to_hex()
    }

    #[wasm_bindgen(setter = hash)]
    pub fn set_hash_from_js_value(&mut self, js_value: JsValue) {
        self.hash = Hash::from_slice(&js_value.try_as_vec_u8().expect("hash merkle root"));
    }

    #[wasm_bindgen(getter = hashMerkleRoot)]
    pub fn get_hash_merkle_root_as_hex(&self) -> String {
        self.hash_merkle_root.to_hex()
    }

    #[wasm_bindgen(setter = hashMerkleRoot)]
    pub fn set_hash_merkle_root_from_js_value(&mut self, js_value: JsValue) {
        self.hash_merkle_root = Hash::from_slice(&js_value.try_as_vec_u8().expect("hash merkle root"));
    }

    #[wasm_bindgen(getter = acceptedIdMerkleRoot)]
    pub fn get_accepted_id_merkle_root_as_hex(&self) -> String {
        self.accepted_id_merkle_root.to_hex()
    }

    #[wasm_bindgen(setter = acceptedIdMerkleRoot)]
    pub fn set_accepted_id_merkle_root_from_js_value(&mut self, js_value: JsValue) {
        self.accepted_id_merkle_root = Hash::from_slice(&js_value.try_as_vec_u8().expect("accepted id merkle root"));
    }

    #[wasm_bindgen(getter = utxoCommitment)]
    pub fn get_utxo_commitment_as_hex(&self) -> String {
        self.utxo_commitment.to_hex()
    }

    #[wasm_bindgen(setter = utxoCommitment)]
    pub fn set_utxo_commitment_from_js_value(&mut self, js_value: JsValue) {
        self.utxo_commitment = Hash::from_slice(&js_value.try_as_vec_u8().expect("utxo commitment"));
    }

    #[wasm_bindgen(getter = pruningPoint)]
    pub fn get_pruning_point_as_hex(&self) -> String {
        self.pruning_point.to_hex()
    }

    #[wasm_bindgen(setter = pruningPoint)]
    pub fn set_pruning_point_from_js_value(&mut self, js_value: JsValue) {
        self.pruning_point = Hash::from_slice(&js_value.try_as_vec_u8().expect("pruning point"));
    }

    #[wasm_bindgen(getter = parentsByLevel)]
    pub fn get_parents_by_level_as_js_value(&self) -> JsValue {
        to_value(&self.parents_by_level).expect("invalid parents_by_level")
    }

    #[wasm_bindgen(setter = parentsByLevel)]
    pub fn set_parents_by_level_from_js_value(&mut self, js_value: JsValue) {
        let array = Array::from(&js_value);
        self.parents_by_level = array
        // .get_vec("parentsByLevel").expect("parentsByLevel is not a vec")
        .iter()
        .map(|jsv| {
            Array::from(&jsv)
                .to_vec()
                .into_iter()
                .map(|hash| Ok(hash.try_into()?))
                .collect::<std::result::Result<Vec<Hash>, Error>>()
        })
        .collect::<std::result::Result<Vec<Vec<Hash>>, Error>>().unwrap_or_else(|err| {
            panic!("{}", err);
        });
    }

    #[wasm_bindgen(getter = blueWork)]
    pub fn blue_work(&self) -> js_sys::BigInt {
        self.blue_work.try_into().unwrap_or_else(|err| panic!("invalid blue work: {err}"))
    }

    #[wasm_bindgen(js_name = getBlueWorkAsHex)]
    pub fn get_blue_work_as_hex(&self) -> String {
        self.blue_work.to_hex()
    }

    #[wasm_bindgen(setter = blueWork)]
    pub fn set_blue_work_from_js_value(&mut self, js_value: JsValue) {
        self.blue_work = js_value.try_into().unwrap_or_else(|err| panic!("invalid blue work: {err}"));
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),
    #[error("{0}")]
    SerdeWasmBindgen(#[from] serde_wasm_bindgen::Error),
    #[error(transparent)]
    WorkflowWasm(#[from] workflow_wasm::error::Error),
    #[error("`TryFrom<JsValue> for Header` - error converting property `{0}`: {1}")]
    Conversion(&'static str, String),
}

impl Error {
    pub fn custom<S: Into<String>>(msg: S) -> Self {
        Self::Custom(msg.into())
    }

    pub fn convert<S: std::fmt::Display>(prop: &'static str, msg: S) -> Self {
        Self::Conversion(prop, msg.to_string())
    }
}

impl TryFrom<JsValue> for Header {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&js_value) {
            let parents_by_level = object
                .get_vec("parentsByLevel")?
                .iter()
                .map(|jsv| {
                    Array::from(jsv)
                        .to_vec()
                        .into_iter()
                        .map(|hash| Ok(hash.try_into()?))
                        .collect::<std::result::Result<Vec<Hash>, Error>>()
                })
                .collect::<std::result::Result<Vec<Vec<Hash>>, Error>>()?;

            let header = Self {
                hash: object.get_value("hash")?.try_into().unwrap_or_default(),
                version: object.get_u16("version")?,
                parents_by_level,
                hash_merkle_root: object
                    .get_value("hashMerkleRoot")?
                    .try_into()
                    .map_err(|err| Error::convert("hashMerkleRoot", err))?,
                accepted_id_merkle_root: object
                    .get_value("acceptedIdMerkleRoot")?
                    .try_into()
                    .map_err(|err| Error::convert("acceptedIdMerkleRoot", err))?,
                utxo_commitment: object
                    .get_value("utxoCommitment")?
                    .try_into()
                    .map_err(|err| Error::convert("utxoCommitment", err))?,
                nonce: object.get_u64("nonce")?,
                timestamp: object.get_u64("timestamp")?,
                daa_score: object.get_u64("daaScore")?,
                bits: object.get_u32("bits")?,
                blue_work: object.get_value("blueWork")?.try_into().map_err(|err| Error::convert("blueWork", err))?,
                blue_score: object.get_u64("blueScore")?,
                pruning_point: object.get_value("pruningPoint")?.try_into().map_err(|err| Error::convert("pruningPoint", err))?,
            };

            Ok(header)
        } else {
            Err(Error::Custom("supplied argument must be an object".to_string()))
        }
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
