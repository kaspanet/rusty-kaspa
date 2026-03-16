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

#[wasm_bindgen(typescript_custom_section)]
const TS_COVENANT_AUTHORIZED_OUTPUT: &'static str = r#"
/**
 * An output authorized by the genesis outpoint for covenant ID derivation.
 *
 * @category Consensus
 */
export interface ICovenantAuthorizedOutput {
    index: number;
    output: ITransactionOutput | TransactionOutput;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type representing an array of covenant authorized outputs.
    ///
    /// @category Consensus
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "ICovenantAuthorizedOutput[]")]
    pub type CovenantAuthorizedOutputArrayT;
}

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
pub fn js_covenant_id(genesis_outpoint: &TransactionOutpointT, auth_outputs: &CovenantAuthorizedOutputArrayT) -> Result<native::Hash> {
    let outpoint_client = TransactionOutpoint::try_from(genesis_outpoint.as_ref())?;
    let outpoint = cctx::TransactionOutpoint::from(&outpoint_client);
    let outputs: Vec<(u32, cctx::TransactionOutput)> = auth_outputs
        .iter()
        .map(|item| {
            let obj = js_sys::Object::try_from(&item).ok_or_else(|| Error::custom("each auth_output must be an object"))?;
            let index = obj.get_u32("index")?;
            let output = TransactionOutput::try_owned_from(obj.get_value("output")?)?;
            Ok((index, cctx::TransactionOutput::from(&output)))
        })
        .collect::<Result<_>>()?;

    Ok(covenant_id::covenant_id(outpoint, outputs.iter().map(|(i, o)| (*i, o))))
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::output::TransactionOutput;
    use kaspa_consensus_core::hashing::covenant_id;
    use kaspa_consensus_core::tx::ScriptPublicKey;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::wasm_bindgen_test;

    // Helper - construct ScriptPublicKey
    fn construct_spk() -> ScriptPublicKey {
        ScriptPublicKey::new(0, vec![0xaa, 0xbb].into())
    }

    // Helper - construct plain TransactionOutpoint JS object
    fn construct_outpoint_obj(index: u32) -> Object {
        let txid_hex = format!("{}", TransactionId::from_slice(&[0xab; 32]));
        let obj = Object::new();
        obj.set("transactionId", &JsValue::from_str(&txid_hex)).unwrap();
        obj.set("index", &JsValue::from(index)).unwrap();
        obj
    }

    // Helper - construct ICovenantAuthorizedOutput JS object
    fn construct_auth_output_obj(index: u32, value: u64, spk: &ScriptPublicKey) -> JsValue {
        let obj = Object::new();
        obj.set("index", &JsValue::from(index)).unwrap();
        let output = TransactionOutput::ctor(value, spk, None);
        obj.set("output", &JsValue::from(output)).unwrap();
        obj.into()
    }

    // Helper - construct ICovenantAuthorizedOutput[] JS array
    fn construct_auth_outputs_array(entries: &[(u32, u64)]) -> js_sys::Array {
        let spk = construct_spk();
        let arr = js_sys::Array::new();
        for &(index, value) in entries {
            arr.push(&construct_auth_output_obj(index, value, &spk));
        }
        arr
    }

    #[wasm_bindgen_test]
    fn test_covenant_id_matches_core() {
        let spk = construct_spk();
        let entries: &[(u32, u64)] = &[(0, 1000), (1, 2000)];

        // WASM covenant id
        let outpoint_js = construct_outpoint_obj(5);
        let auth_array = construct_auth_outputs_array(entries);
        let wasm_hash = js_covenant_id(outpoint_js.unchecked_ref(), auth_array.unchecked_ref()).expect("wasm call should succeed");

        // Core covenant id
        let core_outpoint = cctx::TransactionOutpoint::new(TransactionId::from_slice(&[0xab; 32]), 5);
        let core_outputs: Vec<(u32, cctx::TransactionOutput)> =
            entries.iter().map(|&(i, v)| (i, cctx::TransactionOutput::new(v, spk.clone()))).collect();
        let core_hash = covenant_id::covenant_id(core_outpoint, core_outputs.iter().map(|(i, o)| (*i, o)));

        assert_eq!(wasm_hash, core_hash);
    }

    #[wasm_bindgen_test]
    fn test_covenant_id_rejects_non_object_in_array() {
        let outpoint = construct_outpoint_obj(0);
        let arr = js_sys::Array::new();
        arr.push(&JsValue::from(42));
        let result = js_covenant_id(outpoint.unchecked_ref(), arr.unchecked_ref());
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    fn test_covenant_id_rejects_missing_fields() {
        let outpoint = construct_outpoint_obj(0);
        let arr = js_sys::Array::new();
        let obj = Object::new();
        obj.set("index", &JsValue::from(0)).unwrap();
        // intentionally omit "output" field
        arr.push(&obj.into());
        let result = js_covenant_id(outpoint.unchecked_ref(), arr.unchecked_ref());
        assert!(result.is_err());
    }
}
