use crate::matrix::Matrix;
use js_sys::BigInt;
use kaspa_consensus_client::Header;
use kaspa_consensus_client::HeaderT;
use kaspa_consensus_core::hashing;
use kaspa_hashes::Hash;
use kaspa_hashes::PowHash;
use kaspa_math::Uint256;
use kaspa_utils::hex::FromHex;
use kaspa_utils::hex::ToHex;
use num::Float;
use wasm_bindgen::prelude::*;
use workflow_wasm::convert::TryCastFromJs;
use workflow_wasm::error::Error;
use workflow_wasm::result::Result;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "[boolean, bigint]")]
    pub type WorkT;
}

/// Represents a Kaspa header PoW manager
/// @category Mining
#[wasm_bindgen(inspectable)]
pub struct PoW {
    inner: crate::State,
    pre_pow_hash: Hash,
}

#[wasm_bindgen]
impl PoW {
    #[wasm_bindgen(constructor)]
    pub fn new(header: &HeaderT, timestamp: Option<u64>) -> Result<PoW> {
        // this function replicates crate::State::new() but caches
        // the pre_pow_hash value internally, making it available
        // via the `pre_pow_hash` property getter.
        let header = Header::try_cast_from(header).map_err(Error::custom)?;
        let header = header.as_ref();
        let header = header.inner();

        // Get required target from header bits.
        let target = Uint256::from_compact_target_bits(header.bits);
        // Zero out the time and nonce.
        let pre_pow_hash = hashing::header::hash_override_nonce_time(header, 0, 0);
        // PRE_POW_HASH || TIME || 32 zero byte padding || NONCE
        let hasher = PowHash::new(pre_pow_hash, timestamp.unwrap_or(header.timestamp));
        let matrix = Matrix::generate(pre_pow_hash);

        Ok(Self { inner: crate::State { matrix, target, hasher }, pre_pow_hash })
    }

    /// The target based on the provided bits.
    #[wasm_bindgen(getter)]
    pub fn target(&self) -> Result<BigInt> {
        self.inner.target.try_into().map_err(|err| Error::custom(format!("{err:?}")))
    }

    /// Checks if the computed target meets or exceeds the difficulty specified in the template.
    /// @returns A boolean indicating if it reached the target and a bigint representing the reached target.
    #[wasm_bindgen(js_name=checkWork)]
    pub fn check_work(&self, nonce: u64) -> Result<WorkT> {
        let (c, v) = self.inner.check_pow(nonce);
        let array = js_sys::Array::new();
        array.push(&JsValue::from(c));
        array.push(&v.to_bigint().map_err(|err| Error::custom(format!("{err:?}")))?.into());

        Ok(array.unchecked_into())
    }

    /// Hash of the header without timestamp and nonce.
    #[wasm_bindgen(getter = prePoWHash)]
    pub fn get_pre_pow_hash(&self) -> String {
        self.pre_pow_hash.to_hex()
    }

    /// Can be used for parsing Stratum templates.
    #[wasm_bindgen(js_name=fromRaw)]
    pub fn from_raw(pre_pow_hash: &str, timestamp: u64, target_bits: Option<u32>) -> Result<PoW> {
        // Convert the pre_pow_hash from hex string to Hash
        let pre_pow_hash = Hash::from_hex(pre_pow_hash).map_err(|err| Error::custom(format!("{err:?}")))?;

        // Generate the target from compact target bits if provided
        let target = Uint256::from_compact_target_bits(target_bits.unwrap_or_default());

        // Initialize the matrix and hasher using pre_pow_hash and timestamp
        let matrix = Matrix::generate(pre_pow_hash);
        let hasher = PowHash::new(pre_pow_hash, timestamp);

        Ok(PoW { inner: crate::State { matrix, target, hasher }, pre_pow_hash })
    }
}

// https://github.com/tmrlvi/kaspa-miner/blob/bf361d02a46c580f55f46b5dfa773477634a5753/src/client/stratum.rs#L36
const DIFFICULTY_1_TARGET: (u64, i16) = (0xffffu64, 208); // 0xffff 2^208

/// Calculates target from difficulty, based on set_difficulty function on
/// <https://github.com/tmrlvi/kaspa-miner/blob/bf361d02a46c580f55f46b5dfa773477634a5753/src/client/stratum.rs#L375>
/// @category Mining
#[wasm_bindgen(js_name = calculateTarget)]
pub fn calculate_target(difficulty: f32) -> Result<BigInt> {
    let mut buf = [0u64, 0u64, 0u64, 0u64];
    let (mantissa, exponent, _) = difficulty.recip().integer_decode();
    let new_mantissa = mantissa * DIFFICULTY_1_TARGET.0;
    let new_exponent = (DIFFICULTY_1_TARGET.1 + exponent) as u64;
    let start = (new_exponent / 64) as usize;
    let remainder = new_exponent % 64;

    buf[start] = new_mantissa << remainder; // bottom
    if start < 3 {
        buf[start + 1] = new_mantissa >> (64 - remainder); // top
    } else if new_mantissa.leading_zeros() < remainder as u32 {
        return Err(Error::custom("Target is too big"));
    }

    Uint256(buf).try_into().map_err(Error::custom)
}
