use crate::zk_to_script::wasm::{InnerState, ZkScriptBuilder, into_array_32};
use kaspa_txscript::error::Error;
use kaspa_txscript::result::Result;
use kaspa_txscript::zk_precompiles::risc0::rcpt::HashFnId;
use kaspa_wasm_core::types::BinaryT;
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

fn parse_hash_fn_id(value: &str) -> Result<HashFnId> {
    match value {
        "blake2b" => Ok(HashFnId::Blake2b),
        "poseidon2" => Ok(HashFnId::Poseidon2),
        "sha-256" => Ok(HashFnId::Sha256),
        _ => Err(Error::custom(format!("invalid hash function id: {value}"))),
    }
}

#[wasm_bindgen]
impl ZkScriptBuilder {
    /// Commits the script to unlocking only on a valid succinct proof for the
    /// given image id and control id. `hashFnId` accepts "blake2b",
    /// "poseidon2", or "sha-256"; defaults to "poseidon2" when omitted.
    /// Transitions an unbounded builder into the succinct-bounded state.
    #[wasm_bindgen(js_name = "commitToSuccinct")]
    pub fn commit_to_succinct(&mut self, image_id: BinaryT, control_id: BinaryT, hash_fn_id: Option<String>) -> Result<()> {
        let image_id = into_array_32(image_id.try_as_vec_u8()?, "imageId")?;
        let control_id = into_array_32(control_id.try_as_vec_u8()?, "controlId")?;
        let hash_fn = hash_fn_id.as_deref().map(parse_hash_fn_id).transpose()?;

        match self.take() {
            InnerState::Unbounded(b) => {
                let bounded = b.commit_to_succinct(image_id, control_id, hash_fn).map_err(|e| Error::custom(e.to_string()))?;
                self.inner = InnerState::BoundedSuccinct(bounded);
                Ok(())
            }
            other => {
                self.inner = other;
                Err(Error::custom("commitToSuccinct requires an unbounded builder"))
            }
        }
    }
}
