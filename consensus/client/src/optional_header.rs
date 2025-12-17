//!
//! WASM implementation of a partial/optional Header.
//! This struct is composed of primitives and other WASM types.
//!

use crate::error::Error;
use crate::imports::*;
use crate::parents::CompressedParents as WasmCompressedParents;
use js_sys::Object;
use kaspa_consensus_core::BlueWorkType;
use kaspa_hashes::Hash;
use kaspa_utils::hex::ToHex;
use workflow_wasm::extensions::{JsValueExtension, ObjectExtension};

#[wasm_bindgen(typescript_custom_section)]
const TS_OPTIONAL_HEADER: &'static str = r#"
/**
 * Represents a block header where all fields are optional.
 *
 * @category Consensus
 */
export interface IOptionalHeader {
    hash?: HexString;
    version?: number;
    parentsByLevel?: CompressedParents;
    hashMerkleRoot?: HexString;
    acceptedIdMerkleRoot?: HexString;
    utxoCommitment?: HexString;
    timestamp?: bigint;
    bits?: number;
    nonce?: bigint;
    daaScore?: bigint;
    blueWork?: bigint | HexString;
    blueScore?: bigint;
    pruningPoint?: HexString;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type definition for OptionalHeader-like data.
    #[wasm_bindgen(typescript_type = "OptionalHeader | IOptionalHeader")]
    pub type OptionalHeaderT;
}

#[wasm_bindgen(inspectable)]
#[derive(Clone, Debug, CastFromJs)]
pub struct OptionalHeader {
    hash: Option<Hash>,
    version: Option<u16>,
    parents_by_level: Option<WasmCompressedParents>,
    hash_merkle_root: Option<Hash>,
    accepted_id_merkle_root: Option<Hash>,
    utxo_commitment: Option<Hash>,
    timestamp: Option<u64>,
    bits: Option<u32>,
    nonce: Option<u64>,
    daa_score: Option<u64>,
    blue_work: Option<BlueWorkType>,
    blue_score: Option<u64>,
    pruning_point: Option<Hash>,
}

impl OptionalHeader {
    #[allow(clippy::too_many_arguments)]
    pub fn new_from_fields(
        hash: Option<Hash>,
        version: Option<u16>,
        parents_by_level: Option<WasmCompressedParents>,
        hash_merkle_root: Option<Hash>,
        accepted_id_merkle_root: Option<Hash>,
        utxo_commitment: Option<Hash>,
        timestamp: Option<u64>,
        bits: Option<u32>,
        nonce: Option<u64>,
        daa_score: Option<u64>,
        blue_work: Option<BlueWorkType>,
        blue_score: Option<u64>,
        pruning_point: Option<Hash>,
    ) -> Self {
        Self {
            hash,
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            timestamp,
            bits,
            nonce,
            daa_score,
            blue_work,
            blue_score,
            pruning_point,
        }
    }
}

#[wasm_bindgen]
impl OptionalHeader {
    #[wasm_bindgen(constructor)]
    pub fn new(js_value: OptionalHeaderT) -> Result<OptionalHeader, JsError> {
        Ok(js_value.try_into_owned()?)
    }

    #[wasm_bindgen(getter, js_name = hash)]
    pub fn hash(&self) -> Option<String> {
        self.hash.map(|h| h.to_hex())
    }

    #[wasm_bindgen(getter)]
    pub fn version(&self) -> Option<u16> {
        self.version
    }

    #[wasm_bindgen(getter, js_name = parentsByLevel)]
    pub fn parents_by_level(&self) -> Option<WasmCompressedParents> {
        self.parents_by_level.clone()
    }

    #[wasm_bindgen(getter, js_name = hashMerkleRoot)]
    pub fn hash_merkle_root(&self) -> Option<String> {
        self.hash_merkle_root.map(|h| h.to_hex())
    }

    #[wasm_bindgen(getter, js_name = acceptedIdMerkleRoot)]
    pub fn accepted_id_merkle_root(&self) -> Option<String> {
        self.accepted_id_merkle_root.map(|h| h.to_hex())
    }

    #[wasm_bindgen(getter, js_name = utxoCommitment)]
    pub fn utxo_commitment(&self) -> Option<String> {
        self.utxo_commitment.map(|h| h.to_hex())
    }

    #[wasm_bindgen(getter)]
    pub fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    #[wasm_bindgen(getter)]
    pub fn bits(&self) -> Option<u32> {
        self.bits
    }

    #[wasm_bindgen(getter)]
    pub fn nonce(&self) -> Option<u64> {
        self.nonce
    }

    #[wasm_bindgen(getter, js_name = daaScore)]
    pub fn daa_score(&self) -> Option<u64> {
        self.daa_score
    }

    #[wasm_bindgen(getter, js_name = blueScore)]
    pub fn blue_score(&self) -> Option<u64> {
        self.blue_score
    }

    #[wasm_bindgen(getter, js_name = pruningPoint)]
    pub fn pruning_point(&self) -> Option<String> {
        self.pruning_point.map(|h| h.to_hex())
    }

    #[wasm_bindgen(getter, js_name = blueWork)]
    pub fn get_blue_work(&self) -> JsValue {
        match &self.blue_work {
            Some(bw) => {
                if let Ok(bi) = bw.try_into() {
                    let bi: js_sys::BigInt = bi;
                    bi.into()
                } else {
                    JsValue::NULL
                }
            }
            None => JsValue::NULL,
        }
    }
}

impl TryCastFromJs for OptionalHeader {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve_cast(value, || {
            if let Some(object) = Object::try_from(value.as_ref()) {
                let parents_by_level = object.try_get_value("parentsByLevel")?.map(|pbl_val| pbl_val.try_into_owned()).transpose()?;

                Ok(OptionalHeader {
                    hash: object
                        .try_get_value("hash")?
                        .map(|v| v.try_into_owned().map_err(|err| Error::convert("hash", err)))
                        .transpose()?,
                    version: object.try_get_value("version")?.map(|v| v.try_as_u16()).transpose()?,
                    parents_by_level,
                    hash_merkle_root: object
                        .try_get_value("hashMerkleRoot")?
                        .map(|v| v.try_into_owned().map_err(|err| Error::convert("hashMerkleRoot", err)))
                        .transpose()?,
                    accepted_id_merkle_root: object
                        .try_get_value("acceptedIdMerkleRoot")?
                        .map(|v| v.try_into_owned().map_err(|err| Error::convert("acceptedIdMerkleRoot", err)))
                        .transpose()?,
                    utxo_commitment: object
                        .try_get_value("utxoCommitment")?
                        .map(|v| v.try_into_owned().map_err(|err| Error::convert("utxoCommitment", err)))
                        .transpose()?,
                    timestamp: object.try_get_value("timestamp")?.map(|v| v.try_as_u64()).transpose()?,
                    bits: object.try_get_value("bits")?.map(|v| v.try_as_u32()).transpose()?,
                    nonce: object.try_get_value("nonce")?.map(|v| v.try_as_u64()).transpose()?,
                    daa_score: object.try_get_value("daaScore")?.map(|v| v.try_as_u64()).transpose()?,
                    blue_work: object
                        .try_get_value("blueWork")?
                        .map(|v| v.try_into().map_err(|err| Error::convert("blueWork", err)))
                        .transpose()?,
                    blue_score: object.try_get_value("blueScore")?.map(|v| v.try_as_u64()).transpose()?,
                    pruning_point: object
                        .try_get_value("pruningPoint")?
                        .map(|v| v.try_into_owned().map_err(|err| Error::convert("pruningPoint", err)))
                        .transpose()?,
                }
                .into())
            } else {
                Err(Error::Custom("supplied argument must be an object".to_string()))
            }
        })
    }
}
