use js_sys::BigInt;
use kaspa_utils::hex::ToHex;
use crate::matrix::Matrix;
use kaspa_consensus_core::{hashing, header::Header};
use kaspa_hashes::Hash;
use kaspa_hashes::PowHash;
use kaspa_math::Uint256;
use wasm_bindgen::prelude::*;
use workflow_wasm::error::Error;
use workflow_wasm::jsvalue::*;
use workflow_wasm::result::Result;

#[wasm_bindgen(inspectable)]
pub struct State {
    inner: crate::State,
    pre_pow_hash: Hash,
}

#[wasm_bindgen]
impl State {
    #[wasm_bindgen(constructor)]
    pub fn new(header: &Header) -> Self {
        // this function replicates crate::State::new() but caches
        // the pre_pow_hash value internally, making it available
        // via the `pre_pow_hash` property getter.

        let target = Uint256::from_compact_target_bits(header.bits);
        // Zero out the time and nonce.
        let pre_pow_hash = hashing::header::hash_override_nonce_time(header, 0, 0);
        // PRE_POW_HASH || TIME || 32 zero byte padding || NONCE
        let hasher = PowHash::new(pre_pow_hash, header.timestamp);
        let matrix = Matrix::generate(pre_pow_hash);

        Self { inner: crate::State { matrix, target, hasher }, pre_pow_hash }
    }

    #[wasm_bindgen(getter)]
    pub fn target(&self) -> Result<BigInt> {
        self.inner.target.try_into().map_err(|err| Error::Custom(format!("{err:?}")))
    }

    #[wasm_bindgen(js_name=checkPow)]
    pub fn check_pow(&self, nonce_jsv: JsValue) -> Result<js_sys::Array> {
        let nonce = nonce_jsv.try_as_u64()?;
        let (c, v) = self.inner.check_pow(nonce);
        let array = js_sys::Array::new();
        array.push(&JsValue::from(c));
        array.push(&v.to_bigint().map_err(|err| Error::Custom(format!("{err:?}")))?.into());

        Ok(array)
    }

    #[wasm_bindgen(getter = prePowHash)]
    pub fn get_pre_pow_hash(&self) -> String {
        self.pre_pow_hash.to_hex()
    }
}
