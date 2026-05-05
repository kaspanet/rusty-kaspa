use crate::error::Error;
use crate::result::Result;
use crate::zk_precompiles::risc0::rcpt::HashFnId;
use crate::zk_precompiles::risc0::zk_to_script::wasm::{InnerState, R0ScriptBuilder, into_array_32};
use kaspa_wasm_core::types::BinaryT;
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
impl R0ScriptBuilder {
    /// Commits the script to unlocking only on a valid succinct proof for the
    /// given image id and control id. `hashFnId` accepts "blake2b",
    /// "poseidon2", or "sha-256"; defaults to "poseidon2" when omitted.
    /// Transitions an unbounded builder into the succinct-bounded state.
    #[wasm_bindgen(js_name = "commitToSuccinct")]
    pub fn commit_to_succinct(&mut self, image_id: BinaryT, control_id: BinaryT, hash_fn_id: Option<String>) -> Result<()> {
        let image_id = into_array_32(image_id.try_as_vec_u8()?, "imageId")?;
        let control_id = into_array_32(control_id.try_as_vec_u8()?, "controlId")?;
        let hash_fn = hash_fn_id.map(|s| HashFnId::try_from(s.as_str()).map_err(|e| Error::custom(e.to_string()))).transpose()?;

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
