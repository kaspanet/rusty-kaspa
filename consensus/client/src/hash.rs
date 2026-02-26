//!
//! WASM bindings for transaction hashers: [`TransactionSigningHash`](native::TransactionSigningHash)
//! and [`TransactionSigningHashECDSA`](native::TransactionSigningHashECDSA).
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::{TransactionOutpoint, TransactionOutpointT, TransactionOutput, result::Result};
use kaspa_consensus_core::hashing::covenant_id;
use kaspa_hashes as native;
use kaspa_hashes::HasherBase;
use kaspa_wasm_core::types::BinaryT;

/// @category Wallet SDK
#[derive(Default, Clone)]
#[wasm_bindgen]
pub struct TransactionSigningHash {
    hasher: native::TransactionSigningHash,
}

#[wasm_bindgen]
impl TransactionSigningHash {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { hasher: native::TransactionSigningHash::new() }
    }

    pub fn update(&mut self, data: BinaryT) -> Result<()> {
        let data = JsValue::from(data).try_as_vec_u8()?;
        self.hasher.update(data);
        Ok(())
    }

    pub fn finalize(&self) -> String {
        self.hasher.clone().finalize().to_string()
    }
}

/// @category Wallet SDK
#[derive(Default, Clone)]
#[wasm_bindgen]
pub struct TransactionSigningHashECDSA {
    hasher: native::TransactionSigningHashECDSA,
}

#[wasm_bindgen]
impl TransactionSigningHashECDSA {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { hasher: native::TransactionSigningHashECDSA::new() }
    }

    pub fn update(&mut self, data: BinaryT) -> Result<()> {
        let data = JsValue::from(data).try_as_vec_u8()?;
        self.hasher.update(data);
        Ok(())
    }

    pub fn finalize(&self) -> String {
        self.hasher.clone().finalize().to_string()
    }
}

/// Computes the covenant ID from the genesis outpoint and its authorized outputs.
///
/// `genesis_outpoint` may be a [`TransactionOutpoint`] instance or a
/// compatible plain object: `{ transactionId: HexString, index: number }`.
///
/// `auth_outputs` is a JS array of objects, each with:
/// - `index: number` — position of this output in the transaction's output array
/// - `output: TransactionOutput | ITransactionOutput` — the authorized output
///
/// @category Consensus
#[wasm_bindgen(js_name = covenantId)]
pub fn js_covenant_id(genesis_outpoint: &TransactionOutpointT, auth_outputs: Vec<JsValue>) -> Result<native::Hash> {
    let outpoint_client = TransactionOutpoint::try_from(genesis_outpoint.as_ref())?;
    let outpoint = cctx::TransactionOutpoint::from(&outpoint_client);
    let outputs: Vec<(u32, cctx::TransactionOutput)> = auth_outputs
        .iter()
        .map(|item| {
            let obj = js_sys::Object::try_from(item).ok_or_else(|| Error::custom("each auth_output must be an object"))?;
            let index = obj.get_u32("index")?;
            let output = TransactionOutput::try_owned_from(obj.get_value("output")?)?;
            Ok((index, cctx::TransactionOutput::from(&output)))
        })
        .collect::<Result<_>>()?;

    Ok(covenant_id::covenant_id(outpoint, outputs.iter().map(|(i, o)| (*i, o))))
}
