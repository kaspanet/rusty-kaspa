use crate::error::Error;
use crate::result::Result;
use crate::zk_precompiles::risc0::zk_to_script::wasm::{InnerState, R0ScriptBuilder, into_array_32};
use kaspa_wasm_core::types::{BinaryT, HexString};
use risc0_zkvm::{Groth16Receipt, ReceiptClaim};
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
impl R0ScriptBuilder {
    /// Finalizes a Groth16-bounded script with a borsh-encoded
    /// `Groth16Receipt<ReceiptClaim>` and a 32-byte journal hash. Returns the
    /// finalized script bytes as a hex string and consumes the builder.
    #[wasm_bindgen(js_name = "finalizeWithGroth16Proof")]
    pub fn finalize_with_groth16_proof(&mut self, receipt: BinaryT, journal_hash: BinaryT) -> Result<HexString> {
        let receipt_bytes = receipt.try_as_vec_u8()?;
        let journal_hash = into_array_32(journal_hash.try_as_vec_u8()?, "journalHash")?;
        let receipt: Groth16Receipt<ReceiptClaim> =
            borsh::from_slice(&receipt_bytes).map_err(|e| Error::custom(format!("failed to decode Groth16 receipt: {e}")))?;

        match self.take() {
            InnerState::BoundedGroth16(b) => {
                let bytes = b.finalize_with_proof(receipt, journal_hash).map_err(|e| Error::custom(e.to_string()))?;
                Ok(HexString::from(bytes.as_slice()))
            }
            other => {
                self.inner = other;
                Err(Error::custom("finalizeWithGroth16Proof requires a Groth16-bounded builder"))
            }
        }
    }
}
