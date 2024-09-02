use crate::error::Error;
use js_sys::{Array, Object};
use kaspa_consensus_core::hashing;
use kaspa_consensus_core::header as native;
use kaspa_hashes::Hash;
use kaspa_utils::hex::ToHex;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::prelude::{JsError, JsValue};
use workflow_wasm::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_HEADER: &'static str = r#"
/**
 * Interface defining the structure of a block header.
 * 
 * @category Consensus
 */
export interface IHeader {
    hash: HexString;
    version: number;
    parentsByLevel: Array<Array<HexString>>;
    hashMerkleRoot: HexString;
    acceptedIdMerkleRoot: HexString;
    utxoCommitment: HexString;
    timestamp: bigint;
    bits: number;
    nonce: bigint;
    daaScore: bigint;
    blueWork: bigint | HexString;
    blueScore: bigint;
    pruningPoint: HexString;
}

/**
 * Interface defining the structure of a raw block header.
 * 
 * This interface is explicitly used by GetBlockTemplate and SubmitBlock RPCs
 * and unlike `IHeader`, does not include a hash.
 * 
 * @category Consensus
 */
export interface IRawHeader {
    version: number;
    parentsByLevel: Array<Array<HexString>>;
    hashMerkleRoot: HexString;
    acceptedIdMerkleRoot: HexString;
    utxoCommitment: HexString;
    timestamp: bigint;
    bits: number;
    nonce: bigint;
    daaScore: bigint;
    blueWork: bigint | HexString;
    blueScore: bigint;
    pruningPoint: HexString;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Header | IHeader | IRawHeader")]
    pub type HeaderT;
}

/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct Header {
    inner: native::Header,
}

impl Header {
    #[inline]
    pub fn inner(&self) -> &native::Header {
        &self.inner
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut native::Header {
        &mut self.inner
    }
}

#[cfg(feature = "wasm32-sdk")]
#[wasm_bindgen]
impl Header {
    #[wasm_bindgen(constructor)]
    pub fn constructor(js_value: HeaderT) -> std::result::Result<Header, JsError> {
        Ok(js_value.try_into_owned()?)
    }

    /// Finalizes the header and recomputes (updates) the header hash
    /// @return { String } header hash
    #[wasm_bindgen(js_name = finalize)]
    pub fn finalize_js(&mut self) -> String {
        // let inner = self.inner.lock().unwrap();
        let inner = self.inner_mut();
        inner.hash = hashing::header::hash(inner);
        inner.hash.to_hex()
    }

    /// Obtain `JSON` representation of the header. JSON representation
    /// should be obtained using WASM, to ensure proper serialization of
    /// big integers.
    #[wasm_bindgen(js_name = asJSON)]
    pub fn as_json(&self) -> String {
        serde_json::to_string(self.inner()).unwrap()
    }

    #[wasm_bindgen(getter = version)]
    pub fn get_version(&self) -> u16 {
        self.inner().version
    }

    #[wasm_bindgen(setter = version)]
    pub fn set_version(&mut self, version: u16) {
        self.inner_mut().version = version
    }

    #[wasm_bindgen(getter = timestamp)]
    pub fn get_timestamp(&self) -> u64 {
        self.inner().timestamp
    }

    #[wasm_bindgen(setter = timestamp)]
    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.inner_mut().timestamp = timestamp
    }

    #[wasm_bindgen(getter = bits)]
    pub fn bits(&self) -> u32 {
        self.inner().bits
    }

    #[wasm_bindgen(setter = bits)]
    pub fn set_bits(&mut self, bits: u32) {
        self.inner_mut().bits = bits
    }

    #[wasm_bindgen(getter = nonce)]
    pub fn nonce(&self) -> u64 {
        self.inner().nonce
    }

    #[wasm_bindgen(setter = nonce)]
    pub fn set_nonce(&mut self, nonce: u64) {
        self.inner_mut().nonce = nonce
    }

    #[wasm_bindgen(getter = daaScore)]
    pub fn daa_score(&self) -> u64 {
        self.inner().daa_score
    }

    #[wasm_bindgen(setter = daaScore)]
    pub fn set_daa_score(&mut self, daa_score: u64) {
        self.inner_mut().daa_score = daa_score
    }

    #[wasm_bindgen(getter = blueScore)]
    pub fn blue_score(&self) -> u64 {
        self.inner().blue_score
    }

    #[wasm_bindgen(setter = blueScore)]
    pub fn set_blue_score(&mut self, blue_score: u64) {
        self.inner_mut().blue_score = blue_score
    }

    #[wasm_bindgen(getter = hash)]
    pub fn get_hash_as_hex(&self) -> String {
        self.inner().hash.to_hex()
    }

    #[wasm_bindgen(getter = hashMerkleRoot)]
    pub fn get_hash_merkle_root_as_hex(&self) -> String {
        self.inner().hash_merkle_root.to_hex()
    }

    #[wasm_bindgen(setter = hashMerkleRoot)]
    pub fn set_hash_merkle_root_from_js_value(&mut self, js_value: JsValue) {
        self.inner_mut().hash_merkle_root = Hash::from_slice(&js_value.try_as_vec_u8().expect("hash merkle root"));
    }

    #[wasm_bindgen(getter = acceptedIdMerkleRoot)]
    pub fn get_accepted_id_merkle_root_as_hex(&self) -> String {
        self.inner().accepted_id_merkle_root.to_hex()
    }

    #[wasm_bindgen(setter = acceptedIdMerkleRoot)]
    pub fn set_accepted_id_merkle_root_from_js_value(&mut self, js_value: JsValue) {
        self.inner_mut().accepted_id_merkle_root = Hash::from_slice(&js_value.try_as_vec_u8().expect("accepted id merkle root"));
    }

    #[wasm_bindgen(getter = utxoCommitment)]
    pub fn get_utxo_commitment_as_hex(&self) -> String {
        self.inner().utxo_commitment.to_hex()
    }

    #[wasm_bindgen(setter = utxoCommitment)]
    pub fn set_utxo_commitment_from_js_value(&mut self, js_value: JsValue) {
        self.inner_mut().utxo_commitment = Hash::from_slice(&js_value.try_as_vec_u8().expect("utxo commitment"));
    }

    #[wasm_bindgen(getter = pruningPoint)]
    pub fn get_pruning_point_as_hex(&self) -> String {
        self.inner().pruning_point.to_hex()
    }

    #[wasm_bindgen(setter = pruningPoint)]
    pub fn set_pruning_point_from_js_value(&mut self, js_value: JsValue) {
        self.inner_mut().pruning_point = Hash::from_slice(&js_value.try_as_vec_u8().expect("pruning point"));
    }

    #[wasm_bindgen(getter = parentsByLevel)]
    pub fn get_parents_by_level_as_js_value(&self) -> JsValue {
        to_value(&self.inner().parents_by_level).expect("invalid parents_by_level")
    }

    #[wasm_bindgen(setter = parentsByLevel)]
    pub fn set_parents_by_level_from_js_value(&mut self, js_value: JsValue) {
        let array = Array::from(&js_value);
        self.inner_mut().parents_by_level = array
            .iter()
            .map(|jsv| {
                Array::from(&jsv)
                    .to_vec()
                    .iter()
                    .map(|hash| Ok(hash.try_into_owned()?))
                    .collect::<std::result::Result<Vec<Hash>, Error>>()
            })
            .collect::<std::result::Result<Vec<Vec<Hash>>, Error>>()
            .unwrap_or_else(|err| {
                panic!("{}", err);
            });
    }

    #[wasm_bindgen(getter = blueWork)]
    pub fn blue_work(&self) -> js_sys::BigInt {
        self.inner().blue_work.try_into().unwrap_or_else(|err| panic!("invalid blue work: {err}"))
    }

    #[wasm_bindgen(js_name = getBlueWorkAsHex)]
    pub fn get_blue_work_as_hex(&self) -> String {
        self.inner().blue_work.to_hex()
    }

    #[wasm_bindgen(setter = blueWork)]
    pub fn set_blue_work_from_js_value(&mut self, js_value: JsValue) {
        self.inner_mut().blue_work = js_value.try_into().unwrap_or_else(|err| panic!("invalid blue work: {err}"));
    }
}

impl TryCastFromJs for Header {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if let Some(object) = Object::try_from(value.as_ref()) {
                let parents_by_level = object
                    .get_vec("parentsByLevel")?
                    .iter()
                    .map(|jsv| {
                        Array::from(jsv)
                            .to_vec()
                            .into_iter()
                            .map(|hash| Ok(hash.try_into_owned()?))
                            .collect::<std::result::Result<Vec<Hash>, Error>>()
                    })
                    .collect::<std::result::Result<Vec<Vec<Hash>>, Error>>()?;

                let header = native::Header {
                    hash: object.get_value("hash")?.try_into_owned().unwrap_or_default(),
                    version: object.get_u16("version")?,
                    parents_by_level,
                    hash_merkle_root: object
                        .get_value("hashMerkleRoot")?
                        .try_into_owned()
                        .map_err(|err| Error::convert("hashMerkleRoot", err))?,
                    accepted_id_merkle_root: object
                        .get_value("acceptedIdMerkleRoot")?
                        .try_into_owned()
                        .map_err(|err| Error::convert("acceptedIdMerkleRoot", err))?,
                    utxo_commitment: object
                        .get_value("utxoCommitment")?
                        .try_into_owned()
                        .map_err(|err| Error::convert("utxoCommitment", err))?,
                    nonce: object.get_u64("nonce")?,
                    timestamp: object.get_u64("timestamp")?,
                    daa_score: object.get_u64("daaScore")?,
                    bits: object.get_u32("bits")?,
                    blue_work: object.get_value("blueWork")?.try_into().map_err(|err| Error::convert("blueWork", err))?,
                    blue_score: object.get_u64("blueScore")?,
                    pruning_point: object
                        .get_value("pruningPoint")?
                        .try_into_owned()
                        .map_err(|err| Error::convert("pruningPoint", err))?,
                };

                Ok(header.into())
            } else {
                Err(Error::Custom("supplied argument must be an object".to_string()))
            }
        })
    }
}

impl From<native::Header> for Header {
    fn from(header: native::Header) -> Self {
        Self { inner: header }
    }
}
