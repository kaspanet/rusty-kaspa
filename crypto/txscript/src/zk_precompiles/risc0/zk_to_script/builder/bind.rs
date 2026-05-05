use ark_bn254::Bn254;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use risc0_zkvm::Digest;
use std::marker::PhantomData;

use super::super::Result;
use crate::{opcodes::codes::OpZkPrecompile, zk_precompiles::{
    risc0::{
        R0Error,
        zk_to_script::{
            BoundedGroth16Script, BoundedR0SuccinctScript, R0ScriptBuilder, UnboundedZkScript, UninitializedZkScript,
            builder::proof::R0_SERIALIZED_UNCOMPRESSED_VK,
        },
    },
    tags::ZkTag,
}};

impl R0ScriptBuilder<UnboundedZkScript> {
    pub fn commit_to_stark(mut self, image_id: Digest) -> Result<R0ScriptBuilder<BoundedR0SuccinctScript>> {
        self.builder.add_data(image_id.as_bytes())?;
        self.builder.add_data(&[ZkTag::R0Succinct as u8])?;
        Ok(R0ScriptBuilder { builder: self.builder, _state: PhantomData })
    }

    pub fn commit_to_groth16(mut self, image_id: Digest) -> Result<R0ScriptBuilder<BoundedGroth16Script>> {
        // Utilize the groth16 R0 VK, which is used to verify
        // r0 circuits such as the lift program.
        let verifying_key = ark_groth16::VerifyingKey::<Bn254>::deserialize_uncompressed(R0_SERIALIZED_UNCOMPRESSED_VK.as_slice())?;
        // Serialize the verifying key in compressed form as well, to save space
        // the verifying key here will never change, and as such it can be used
        // for script specific logic.
        let mut serialized_vk = Vec::new();
        verifying_key.serialize_compressed(&mut serialized_vk).map_err(|_| R0Error::BincodeVkSerialization)?;

        self.builder.add_data(&serialized_vk)?; // the verifying key
        self.builder.add_data(&[ZkTag::Groth16 as u8])?;
        self.builder.add_op(OpZkPrecompile)?;
        self.builder.add_data(image_id.as_bytes())?;

        Ok(R0ScriptBuilder { builder: self.builder, _state: PhantomData })
    }
}
