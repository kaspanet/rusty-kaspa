use crate::zk_to_script::wasm::proof::FinalizedR0Script;
use crate::zk_to_script::wasm::{InnerState, ZkScriptBuilder, into_array_32};
use kaspa_txscript::error::Error;
use kaspa_txscript::result::Result;
use kaspa_wasm_core::types::BinaryT;
use risc0_zkvm::{Groth16Receipt, ReceiptClaim};
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
impl ZkScriptBuilder {
    /// Finalizes a Groth16-bounded script with a borsh-encoded
    /// `Groth16Receipt<ReceiptClaim>` and a 32-byte journal hash. Returns the
    /// spending script and the inner redeem script. If this is a preparation
    /// in order to unlock a ZK-locked UTXO the script is now ready.
    #[wasm_bindgen(js_name = "finalizeWithGroth16Proof")]
    pub fn finalize_with_groth16_proof(&mut self, receipt: BinaryT, journal_hash: BinaryT) -> Result<FinalizedR0Script> {
        let receipt_bytes = receipt.try_as_vec_u8()?;
        let journal_hash = into_array_32(journal_hash.try_as_vec_u8()?, "journalHash")?;
        let receipt: Groth16Receipt<ReceiptClaim> =
            borsh::from_slice(&receipt_bytes).map_err(|e| Error::custom(format!("failed to decode Groth16 receipt: {e}")))?;

        match self.take() {
            InnerState::BoundedGroth16(b) => {
                let finalized = b.finalize_with_proof(receipt, journal_hash).map_err(|e| Error::custom(e.to_string()))?;
                Ok(finalized.into())
            }
            other => {
                self.inner = other;
                Err(Error::custom("finalizeWithGroth16Proof requires a Groth16-bounded builder"))
            }
        }
    }
}
