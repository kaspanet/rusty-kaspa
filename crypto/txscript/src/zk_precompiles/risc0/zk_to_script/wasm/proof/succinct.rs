use crate::error::Error;
use crate::result::Result;
use crate::zk_precompiles::risc0::zk_to_script::wasm::proof::FinalizedR0Script;
use crate::zk_precompiles::risc0::zk_to_script::wasm::{InnerState, R0ScriptBuilder};
use kaspa_wasm_core::types::BinaryT;
use risc0_zkvm::{Digest, ReceiptClaim, SuccinctReceipt};
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
impl R0ScriptBuilder {
    /// Finalizes a succinct-bounded script with a borsh-encoded
    /// `SuccinctReceipt<ReceiptClaim>` and a 32-byte journal digest. If this is a preparation
    /// in order to unlock a ZK-locked UTXO the script is now ready.
    #[wasm_bindgen(js_name = "finalizeWithSuccinctProof")]
    pub fn finalize_with_succinct_proof(&mut self, receipt: BinaryT, journal: BinaryT) -> Result<FinalizedR0Script> {
        let receipt_bytes = receipt.try_as_vec_u8()?;
        let journal_bytes = journal.try_as_vec_u8()?;
        let journal_digest: Digest =
            journal_bytes.as_slice().try_into().map_err(|_| Error::custom("journal must be 32 bytes"))?;
        let receipt: SuccinctReceipt<ReceiptClaim> =
            borsh::from_slice(&receipt_bytes).map_err(|e| Error::custom(format!("failed to decode succinct receipt: {e}")))?;

        match self.take() {
            InnerState::BoundedSuccinct(b) => {
                let finalized = b.finalize_with_proof(receipt, journal_digest).map_err(|e| Error::custom(e.to_string()))?;
                Ok(finalized.into())
            }
            other => {
                self.inner = other;
                Err(Error::custom("finalizeWithSuccinctProof requires a succinct-bounded builder"))
            }
        }
    }
}
