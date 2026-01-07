mod benchmarks;
mod error;
mod fields;
mod groth16;
pub mod risc0;
pub mod tags;
use crate::{
    data_stack::Stack,
    zk_precompiles::{error::ZkIntegrityError, groth16::Groth16Precompile, risc0::R0SuccinctPrecompile, tags::ZkTag},
};
use kaspa_txscript_errors::TxScriptError;

trait ZkPrecompile {
    type Error: Into<ZkIntegrityError> + std::fmt::Display;
    fn verify_zk(dstack: &mut Stack) -> Result<(), Self::Error>;
}

pub(crate) fn parse_tag(dstack: &mut Stack) -> Result<ZkTag, TxScriptError> {
    let [tag_bytes] = dstack.pop_raw()?;
    ZkTag::try_from(*tag_bytes.first().ok_or(TxScriptError::ZkIntegrity("Could not find tag byte".to_string()))?)
        .map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))
}

/**
 * Verifies a ZK proof from the data stack.
 * The first byte on the stack indicates the ZK tag (proof type).
 */
pub(crate) fn verify_zk(tag: ZkTag, dstack: &mut Stack) -> Result<(), TxScriptError> {
    // Matcth the tag and verify the proof accordingly
    match tag {
        ZkTag::Groth16 => Groth16Precompile::verify_zk(dstack).map_err(|e| TxScriptError::ZkIntegrity(e.to_string())),
        ZkTag::R0Succinct => R0SuccinctPrecompile::verify_zk(dstack).map_err(|e| TxScriptError::ZkIntegrity(e.to_string())),
    }
}

/**
 * A helper function to compute the sigop cost of a ZK proof based on its tag.
 */
pub fn compute_zk_sigop_cost(tag: u8) -> u16 {
    ZkTag::try_from(tag).map(|t| t.sigop_cost()).unwrap_or(ZkTag::max_cost()) // Default to highest cost for unknown tags
}
