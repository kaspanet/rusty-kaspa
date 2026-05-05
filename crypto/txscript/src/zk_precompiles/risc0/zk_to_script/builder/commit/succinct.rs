use risc0_zkvm::Digest;
use std::marker::PhantomData;

use super::super::super::Result;
use crate::{
    opcodes::codes::OpZkPrecompile,
    zk_precompiles::{
        risc0::{
            rcpt::HashFnId,
            zk_to_script::{BoundedR0SuccinctScript, R0ScriptBuilder, UnboundedR0Script},
        },
        tags::ZkTag,
    },
};

impl R0ScriptBuilder<UnboundedR0Script> {
    /// Commits to the succinct proof system,
    /// now the locking script will expect a successful verification
    /// of a succinct proof from the specified image id from the
    /// specified control id and hash function.
    pub fn commit_to_succinct(
        mut self,
        image_id: [u8; 32],
        control_id: [u8; 32],
        hash_fn_id: Option<HashFnId>,
    ) -> Result<R0ScriptBuilder<BoundedR0SuccinctScript>> {
        // Add the image id which is the identifier of the program
        self.builder.add_data(&image_id)?;

        // Add the identifier of which r0 circuit was executed.
        self.builder.add_data(&control_id)?;
        // The hash function id is optional, if not provided it will default to Poseidon2
        // which is the default for succinct.
        self.builder.add_data([hash_fn_id.unwrap_or(HashFnId::Poseidon2) as u8].as_slice())?;

        // This is an r0 succinct proof.
        self.builder.add_data(&[ZkTag::R0Succinct as u8])?;
        self.builder.add_op(OpZkPrecompile)?;
        Ok(R0ScriptBuilder { builder: self.builder, _state: PhantomData })
    }
}
