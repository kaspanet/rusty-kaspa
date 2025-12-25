use crate::{
    data_stack::{DataStack, Stack},
    zk_precompiles::{
        ZkPrecompile, error::ZkIntegrityError, risc0::{ inner::Inner, receipt_claim::compute_assert_claim}
    },
};
use kaspa_txscript_errors::TxScriptError;
use risc0_zkp::core::digest::Digest;
mod inner;
mod receipt_claim;
mod merkle;
mod error;
pub struct R0SuccinctPrecompile;
pub use error::R0Error;
impl ZkPrecompile for R0SuccinctPrecompile {
    type Error = R0Error;
    fn verify_zk(dstack: &mut Stack) -> Result<(), Self::Error> {
        let [image_id] = dstack.pop_raw()?;
        let [journal] = dstack.pop_raw()?;
        let [proof_data] = dstack.pop_raw()?;

        let inner: Inner = borsh::from_slice(&proof_data)?;
        let image_id: Digest = Digest::try_from(image_id).map_err(R0Error::Digest)?;
        let journal: Digest = Digest::try_from(journal).map_err(R0Error::Digest)?;
        // Verify that the claim comes from the provided image id
        compute_assert_claim(inner.claim(), image_id, journal).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;
        inner.verify_integrity()
    }
}
