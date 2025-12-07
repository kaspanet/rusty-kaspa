use crate::{
    data_stack::{DataStack, Stack},
    zk_precompiles::{
        error::ZkIntegrityError,
        risc0::{receipt_claim::compute_assert_claim, succinct::inner::Inner, R0IntegrityVerifier},
        ZkPrecompile,
    },
};
use kaspa_txscript_errors::TxScriptError;
use risc0_zkp::core::digest::Digest;
mod inner;

pub struct R0SuccinctPrecompile;

impl ZkPrecompile for R0SuccinctPrecompile {
    fn verify_zk(dstack: &mut Stack) -> Result<(), ZkIntegrityError> {
        let [image_id] = dstack.pop_raw()?;
        let [journal] = dstack.pop_raw()?;
        let [proof_data] = dstack.pop_raw()?;

        let inner: Inner = borsh::from_slice(&proof_data)?;
        let image_id: Digest = Digest::try_from(image_id).map_err(ZkIntegrityError::Digest)?;
        let journal: Digest = Digest::try_from(journal).map_err(ZkIntegrityError::Digest)?;
        // Verify that the claim comes from the provided image id
        compute_assert_claim(inner.claim(), image_id, journal).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;
        inner.verify_integrity()
    }
}
