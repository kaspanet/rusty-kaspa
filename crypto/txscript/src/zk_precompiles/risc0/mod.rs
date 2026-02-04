use crate::{
    data_stack::Stack,
    zk_precompiles::{
        ZkPrecompile,
        risc0::{rcpt::SuccinctReceipt, receipt_claim::compute_assert_claim},
    },
};
use kaspa_txscript_errors::TxScriptError;
pub use risc0_zkp::core::digest::Digest;
mod error;
pub mod merkle;
pub mod rcpt;
pub mod receipt_claim;

pub struct R0SuccinctPrecompile;
pub use error::R0Error;

impl ZkPrecompile for R0SuccinctPrecompile {
    type Error = R0Error;
    /// Verifies the integrity of a RISC0 succinct proof receipt.
    /// Expects the following items on the stack (from top to bottom):
    /// - proof data (bytes)
    /// - journal (bytes)
    /// - image id (bytes)
    fn verify_zk(dstack: &mut Stack) -> Result<(), Self::Error> {
        let [image_id] = dstack.pop_raw()?;
        let [journal] = dstack.pop_raw()?;
        let [proof_data] = dstack.pop_raw()?;

        // Deserialize the receipt
        // TODO(covpp-mainnet): custom serialization
        let rcpt: SuccinctReceipt = borsh::from_slice(&proof_data)?;

        // Convert image_id and journal to Digest, i.e. a hash
        let image_id: Digest = Digest::try_from(image_id).map_err(R0Error::Digest)?;
        let journal: Digest = Digest::try_from(journal).map_err(R0Error::Digest)?;

     
        // We ensure that the proof itself is valid. This checks the internal consistency of the proof.
        // Due to the binding below, we are assured that the proof corresponds to the claimed image id and journal.
        rcpt.verify_integrity()?;

        // Verify that the claim that comes from the SuccinctReceipt matches the computed claim
        // and then verify the integrity of the receipt. If any of these parameters would change:
        // such as claiming that this came from a different image id, or the journal is different,
        // the assertion would fail.
        // 
        // The other case is if we were to tamper with the claim that comes from the receipt itself.
        // If we were to bypass the compute_assert_claim step, then an attacker could modify the claim in the receipt
        // to match whatever they want and just providing some arbitrary proof. This is why this step is crucial.
        // As this step binds that the provided image id and journal are indeed the ones that were used to generate the proof.
        compute_assert_claim(rcpt.claim(), image_id, journal).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;

        Ok(())
    }
}
