use crate::zk_to_script::wasm::{InnerState, ZkScriptBuilder, into_array_32};
use kaspa_txscript::error::Error;
use kaspa_txscript::result::Result;
use kaspa_wasm_core::types::BinaryT;
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
impl ZkScriptBuilder {
    /// Commits the script to unlocking only on a valid Groth16 proof for the
    /// given 32-byte image id. Transitions an unbounded builder into the
    /// Groth16-bounded state.
    #[wasm_bindgen(js_name = "commitToGroth16")]
    pub fn commit_to_groth16(&mut self, image_id: BinaryT) -> Result<()> {
        let image_id = into_array_32(image_id.try_as_vec_u8()?, "imageId")?;
        match self.take() {
            InnerState::Unbounded(b) => {
                let bounded = b.commit_to_groth16(image_id).map_err(|e| Error::custom(e.to_string()))?;
                self.inner = InnerState::BoundedGroth16(bounded);
                Ok(())
            }
            other => {
                self.inner = other;
                Err(Error::custom("commitToGroth16 requires an unbounded builder"))
            }
        }
    }
}
