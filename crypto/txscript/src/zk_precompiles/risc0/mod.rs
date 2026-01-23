use crate::{
    data_stack::Stack,
    zk_precompiles::{
        ZkPrecompile,
        risc0::{rcpt::SuccinctReceipt, receipt_claim::compute_assert_claim},
    },
};
use kaspa_txscript_errors::TxScriptError;
use risc0_zkp::core::digest::Digest;
mod error;
mod merkle;
mod rcpt;
mod receipt_claim;
pub struct R0SuccinctPrecompile;
pub use error::R0Error;
impl ZkPrecompile for R0SuccinctPrecompile {
    type Error = R0Error;
    fn verify_zk(dstack: &mut Stack) -> Result<(), Self::Error> {
        let [image_id] = dstack.pop_raw()?;
        let [journal] = dstack.pop_raw()?;
        let [proof_data] = dstack.pop_raw()?;

        let rcpt: SuccinctReceipt = borsh::from_slice(&proof_data)?;
        let image_id: Digest = Digest::try_from(image_id).map_err(R0Error::Digest)?;
        let journal: Digest = Digest::try_from(journal).map_err(R0Error::Digest)?;
        // Verify that the claim comes from the provided image id
        compute_assert_claim(rcpt.claim(), image_id, journal).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;
        rcpt.verify_integrity()
    }
}
