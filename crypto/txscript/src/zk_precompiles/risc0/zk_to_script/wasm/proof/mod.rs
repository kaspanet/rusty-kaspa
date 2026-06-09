mod groth16;
mod succinct;

use crate::zk_precompiles::risc0::zk_to_script::FinalizedR0Script as NativeFinalizedR0Script;
use kaspa_wasm_core::types::HexString;
use wasm_bindgen::prelude::wasm_bindgen;

/// Result of finalizing an R0ScriptBuilder.
///
/// `sigScript` is the spending script — set this on the transaction input.
/// `redeemScript` is the inner commit script — hash this with
/// `pay_to_script_hash_script` to derive the P2SH script-public-key.
/// @category Consensus
#[wasm_bindgen(inspectable)]
pub struct FinalizedR0Script {
    sig_script: Vec<u8>,
    redeem_script: Vec<u8>,
}

#[wasm_bindgen]
impl FinalizedR0Script {
    #[wasm_bindgen(getter, js_name = "sigScript")]
    pub fn sig_script(&self) -> HexString {
        HexString::from(self.sig_script.as_slice())
    }

    #[wasm_bindgen(getter, js_name = "redeemScript")]
    pub fn redeem_script(&self) -> HexString {
        HexString::from(self.redeem_script.as_slice())
    }
}

impl From<NativeFinalizedR0Script> for FinalizedR0Script {
    fn from(value: NativeFinalizedR0Script) -> Self {
        Self { sig_script: value.sig_script, redeem_script: value.redeem_script }
    }
}
