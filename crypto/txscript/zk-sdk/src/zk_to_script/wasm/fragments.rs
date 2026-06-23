use crate::zk_to_script::wasm::{ZkScriptBuilder, decode_hash_fn_id, into_array_32};
use crate::zk_to_script::{
    append_r0_groth16_verifier, append_r0_groth16_verifier_with_fixed_journal, append_r0_succinct_verifier,
    append_r0_succinct_verifier_with_fixed_journal, prepare_r0_groth16_proof, prepare_r0_succinct_witness, push_r0_groth16_proof,
    push_r0_succinct_witness,
};
use kaspa_txscript::error::Error;
use kaspa_txscript::result::Result;
use kaspa_wasm_core::types::{BinaryT, HexString};
use risc0_zkvm::{Groth16Receipt, ReceiptClaim, SuccinctReceipt};
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

fn decode_groth16_receipt(receipt: BinaryT) -> Result<Groth16Receipt<ReceiptClaim>> {
    let bytes = receipt.try_as_vec_u8()?;
    borsh::from_slice(&bytes).map_err(|e| Error::custom(format!("failed to decode Groth16 receipt: {e}")))
}

fn decode_succinct_receipt(receipt: BinaryT) -> Result<SuccinctReceipt<ReceiptClaim>> {
    let bytes = receipt.try_as_vec_u8()?;
    borsh::from_slice(&bytes).map_err(|e| Error::custom(format!("failed to decode succinct receipt: {e}")))
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

    /// Pushes the r0 succinct witness material (claim, control index, control
    /// digests, seal). The caller-owned `journal` is pushed
    /// afterwards (on top) to form the verifier's pre-stack.
    #[wasm_bindgen(js_name = "pushR0SuccinctWitness")]
    pub fn push_r0_succinct_witness(&mut self, receipt: BinaryT) -> Result<()> {
        let receipt = decode_succinct_receipt(receipt)?;
        push_r0_succinct_witness(self.inner.builder_mut()?, receipt).map_err(|e| Error::custom(e.to_string()))?;
        Ok(())
    }

    /// Appends the r0 succinct verifier fragment for the given image id, control
    /// id and hash function. `hashFnId` currently only accepts "poseidon2"
    /// (also the default when omitted); other hash functions are not yet
    /// supported. Expects `[..., claim, control_index, control_digests, seal, journal]`.
    #[wasm_bindgen(js_name = "appendR0SuccinctVerifier")]
    pub fn append_r0_succinct_verifier(&mut self, image_id: BinaryT, control_id: BinaryT, hash_fn_id: Option<String>) -> Result<()> {
        let image_id = into_array_32(image_id.try_as_vec_u8()?, "imageId")?;
        let control_id = into_array_32(control_id.try_as_vec_u8()?, "controlId")?;
        let hash_fn_id = decode_hash_fn_id(hash_fn_id)?;
        append_r0_succinct_verifier(self.inner.builder_mut()?, image_id, control_id, hash_fn_id)
            .map_err(|e| Error::custom(e.to_string()))?;
        Ok(())
    }

    /// Appends a fixed-journal r0 succinct verifier fragment, binding
    /// verification to `journal` baked into the script. Expects only
    /// `[..., claim, control_index, control_digests, seal]`.
    #[wasm_bindgen(js_name = "appendR0SuccinctVerifierWithFixedJournal")]
    pub fn append_r0_succinct_verifier_with_fixed_journal(
        &mut self,
        image_id: BinaryT,
        control_id: BinaryT,
        hash_fn_id: Option<String>,
        journal: BinaryT,
    ) -> Result<()> {
        let image_id = into_array_32(image_id.try_as_vec_u8()?, "imageId")?;
        let control_id = into_array_32(control_id.try_as_vec_u8()?, "controlId")?;
        let hash_fn_id = decode_hash_fn_id(hash_fn_id)?;
        let journal = into_array_32(journal.try_as_vec_u8()?, "journal")?;
        append_r0_succinct_verifier_with_fixed_journal(self.inner.builder_mut()?, image_id, control_id, hash_fn_id, journal)
            .map_err(|e| Error::custom(e.to_string()))?;
        Ok(())
    }
}

/// The receipt-derived succinct witness items, hex-encoded, in the order the
/// verifier consumes them (the journal is caller-owned and excluded).
#[wasm_bindgen(inspectable)]
pub struct R0SuccinctWitnessParts {
    claim: Vec<u8>,
    control_index: Vec<u8>,
    control_digests: Vec<u8>,
    seal: Vec<u8>,
}

#[wasm_bindgen]
impl R0SuccinctWitnessParts {
    #[wasm_bindgen(getter)]
    pub fn claim(&self) -> HexString {
        HexString::from(self.claim.as_slice())
    }

    #[wasm_bindgen(getter, js_name = "controlIndex")]
    pub fn control_index(&self) -> HexString {
        HexString::from(self.control_index.as_slice())
    }

    #[wasm_bindgen(getter, js_name = "controlDigests")]
    pub fn control_digests(&self) -> HexString {
        HexString::from(self.control_digests.as_slice())
    }

    #[wasm_bindgen(getter)]
    pub fn seal(&self) -> HexString {
        HexString::from(self.seal.as_slice())
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

/// a borsh-encoded `SuccinctReceipt<ReceiptClaim>` to its four
/// on-stack witness byte vectors (hex), without touching a builder.
#[wasm_bindgen(js_name = "prepareR0SuccinctWitness")]
pub fn prepare_r0_succinct_witness_wasm(receipt: BinaryT) -> Result<R0SuccinctWitnessParts> {
    let receipt = decode_succinct_receipt(receipt)?;
    let w = prepare_r0_succinct_witness(&receipt).map_err(|e| Error::custom(e.to_string()))?;
    Ok(R0SuccinctWitnessParts { claim: w.claim, control_index: w.control_index, control_digests: w.control_digests, seal: w.seal })
}
