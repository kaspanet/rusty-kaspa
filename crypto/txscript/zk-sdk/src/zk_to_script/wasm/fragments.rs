use crate::zk_to_script::wasm::{ZkScriptBuilder, into_array_32};
use crate::zk_to_script::{
    append_r0_groth16_verifier, append_r0_groth16_verifier_dynamic_image_id, append_r0_groth16_verifier_with_fixed_journal,
    prepare_r0_groth16_proof, push_r0_groth16_proof,
};
use kaspa_txscript::error::Error;
use kaspa_txscript::result::Result;
use kaspa_wasm_core::types::{BinaryT, HexString};
use risc0_zkvm::{Groth16Receipt, ReceiptClaim};
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

fn decode_groth16_receipt(receipt: BinaryT) -> Result<Groth16Receipt<ReceiptClaim>> {
    let bytes = receipt.try_as_vec_u8()?;
    borsh::from_slice(&bytes).map_err(|e| Error::custom(format!("failed to decode Groth16 receipt: {e}")))
}

#[wasm_bindgen]
impl ZkScriptBuilder {
    /// Pushes data onto the builder (canonical encoding). Use this to push the
    /// caller-owned journal / journal_hash or the redeem script when composing a
    /// script from fragments.
    #[wasm_bindgen(js_name = "addData")]
    pub fn add_data(&mut self, data: BinaryT) -> Result<()> {
        let data = data.try_as_vec_u8()?;
        self.inner.builder_mut()?.add_data(&data)?;
        Ok(())
    }

    /// Converts an R0 Groth16 receipt into the compressed proof bytes expected
    /// by the Kaspa Groth16 verifier and pushes them. Typically called while
    /// building a signature script; the script that invokes
    /// `appendR0Groth16Verifier` is responsible for placing `journalHash` under
    /// this proof before the verifier runs (so it sees
    /// `[..., journalHash, compressed_proof]`).
    #[wasm_bindgen(js_name = "pushR0Groth16Proof")]
    pub fn push_r0_groth16_proof(&mut self, receipt: BinaryT) -> Result<()> {
        let receipt = decode_groth16_receipt(receipt)?;
        push_r0_groth16_proof(self.inner.builder_mut()?, receipt).map_err(|e| Error::custom(e.to_string()))?;
        Ok(())
    }

    /// Appends the r0-over-groth16 verifier fragment for the given 32-byte image
    /// id. Expects `[..., journal_hash, compressed_proof]` on the stack.
    #[wasm_bindgen(js_name = "appendR0Groth16Verifier")]
    pub fn append_r0_groth16_verifier(&mut self, image_id: BinaryT) -> Result<()> {
        let image_id = into_array_32(image_id.try_as_vec_u8()?, "imageId")?;
        append_r0_groth16_verifier(self.inner.builder_mut()?, image_id).map_err(|e| Error::custom(e.to_string()))?;
        Ok(())
    }

    /// Appends the r0-over-groth16 verifier fragment, consuming the image id
    /// from the stack. Expects
    /// `[..., journal_hash, compressed_proof, image_id]` on the stack.
    #[wasm_bindgen(js_name = "appendR0Groth16VerifierDynamicImageId")]
    pub fn append_r0_groth16_verifier_dynamic_image_id(&mut self) -> Result<()> {
        append_r0_groth16_verifier_dynamic_image_id(self.inner.builder_mut()?).map_err(|e| Error::custom(e.to_string()))?;
        Ok(())
    }

    /// Appends a fixed-journal r0-over-groth16 verifier fragment, binding
    /// verification to `journalHash` baked into the script. Expects only
    /// `[..., compressed_proof]` on the stack.
    #[wasm_bindgen(js_name = "appendR0Groth16VerifierWithFixedJournal")]
    pub fn append_r0_groth16_verifier_with_fixed_journal(&mut self, image_id: BinaryT, journal_hash: BinaryT) -> Result<()> {
        let image_id = into_array_32(image_id.try_as_vec_u8()?, "imageId")?;
        let journal_hash = into_array_32(journal_hash.try_as_vec_u8()?, "journalHash")?;
        append_r0_groth16_verifier_with_fixed_journal(self.inner.builder_mut()?, image_id, journal_hash)
            .map_err(|e| Error::custom(e.to_string()))?;
        Ok(())
    }
}

/// a borsh-encoded `Groth16Receipt<ReceiptClaim>` to the
/// compressed ark-groth16 proof bytes (hex), without touching a builder.
#[wasm_bindgen(js_name = "prepareR0Groth16Proof")]
pub fn prepare_r0_groth16_proof_wasm(receipt: BinaryT) -> Result<HexString> {
    let receipt = decode_groth16_receipt(receipt)?;
    let bytes = prepare_r0_groth16_proof(&receipt).map_err(|e| Error::custom(e.to_string()))?;
    Ok(HexString::from(bytes.as_slice()))
}
