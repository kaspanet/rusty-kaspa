use crate::{
    opcodes::codes::OpZkPrecompile,
    script_builder::ScriptBuilder,
    zk_precompiles::risc0::zk_to_script::{BoundedGroth16Script, BoundedR0SuccinctScript, FinalizedZkScript, R0ScriptBuilder},
};

use super::super::Result;
impl R0ScriptBuilder<BoundedGroth16Script> {
    /// Finalizes the script by consuming the builder and returning the finalized script.
    pub fn finalize(self) -> Result<ScriptBuilder> {
        Ok(self.builder)
    }
}
impl R0ScriptBuilder<BoundedR0SuccinctScript> {
    /// Finalizes the script by consuming the builder and returning the finalized script.
    pub fn finalize(mut self) -> Result<ScriptBuilder> {
        self.builder.add_op(OpZkPrecompile)?;

        Ok(self.builder)
    }
}
