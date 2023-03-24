use crate::{error::Error, Result};
use consensus_core::{
    sign::{sign_with_multiple, verify},
    tx::SignableTransaction,
    wasm::{
        signer::{Result as SignerResult, Signer as SignerTrait},
        MutableTransaction,
    },
};
use js_sys::Array;
use kaspa_bip32::SecretKey;
use std::str::FromStr;
use wasm_bindgen::prelude::*;
use workflow_log::log_trace;

#[wasm_bindgen]
pub struct Signer {
    #[allow(dead_code)]
    extended_private_keys: Vec<String>,
}

#[wasm_bindgen]
impl Signer {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(_extended_private_keys: JsValue) -> Result<Signer> {
        let keys: Vec<String> = vec![]; //extended_private_keys.try_into()?;
        Ok(Self { extended_private_keys: keys })
    }
}

impl SignerTrait for Signer {
    fn sign(&self, mtx: SignableTransaction) -> SignerResult {
        Ok(mtx)
    }
}

#[wasm_bindgen]
pub struct PrivateKey {
    inner: SecretKey,
}

#[wasm_bindgen]
impl PrivateKey {
    #[wasm_bindgen(constructor)]
    pub fn new(key: &str) -> Result<PrivateKey> {
        Ok(Self { inner: SecretKey::from_str(key)? })
    }
}

impl TryFrom<JsValue> for PrivateKey {
    type Error = crate::error::Error;
    fn try_from(value: JsValue) -> Result<Self> {
        Self::new(value.as_string().ok_or(Error::String("Invalid SecretKey".to_string()))?.as_str())
    }
}

#[wasm_bindgen(js_name = "signTransaction")]
pub fn sign_transaction(mtx: MutableTransaction, keys: Array, verify_sig: bool) -> std::result::Result<MutableTransaction, JsError> {
    let mut private_keys: Vec<[u8; 32]> = vec![];
    for key in keys.iter() {
        let k = PrivateKey::try_from(key)?;
        log_trace!("SecretKey: {}", k.inner.display_secret());
        private_keys.push(k.inner.secret_bytes());
    }
    let mtx = sign_with_multiple(mtx.into(), private_keys);
    if verify_sig {
        let mtx_clone = mtx.clone();
        log_trace!("mtx_clone: {mtx_clone:#?}");
        let tx_verifiable = mtx_clone.as_verifiable();
        log_trace!("verify...");
        verify(&tx_verifiable)?;
    }
    let mtx = MutableTransaction::try_from(mtx)?;
    Ok(mtx)
}
