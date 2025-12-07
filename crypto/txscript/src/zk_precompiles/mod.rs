mod error;
mod risc0;
mod tags;
use crate::{
    data_stack::{DataStack, Stack},
    zk_precompiles::{
        error::ZkIntegrityError,
        risc0::{groth16::R0Groth16Precompile, succinct::R0SuccinctPrecompile},
        tags::ZkTag,
    },
};
use kaspa_txscript_errors::TxScriptError;

trait ZkPrecompile {
    fn verify_zk(dstack: &mut Stack) -> Result<(), ZkIntegrityError>;
}

/**
 * Verifies a ZK proof from the data stack.
 * The first byte on the stack indicates the ZK tag (proof type).
 */
pub fn verify_zk(dstack: &mut Stack) -> Result<(), TxScriptError> {
    // Retrieve the zk tag
    let [tag_bytes] = dstack.pop_raw()?;

    // Ensure it is a valid tag before proceeding
    let tag = ZkTag::try_from(tag_bytes[0]).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;

    // Matcth the tag and verify the proof accordingly
    match tag {
        ZkTag::R0Groth16 => R0Groth16Precompile::verify_zk(dstack).map_err(|e| TxScriptError::ZkIntegrity(e.to_string())),
        ZkTag::R0Succinct => R0SuccinctPrecompile::verify_zk(dstack).map_err(|e| TxScriptError::ZkIntegrity(e.to_string())),
    }
}

/**
 * A helper function to compute the sigop cost of a ZK proof based on its tag.
 */
pub fn compute_zk_sigop_cost(tag: u8) -> u32 {
    ZkTag::try_from(tag).map(|t| t.sigop_cost()).unwrap_or(ZkTag::max_cost()) // Default to highest cost for unknown tags
}
