mod inner;
use kaspa_txscript_errors::TxScriptError;
use risc0_zkp::core::digest::Digest;

use crate::{
    data_stack::{DataStack, Stack},
    zk_precompiles::{
        error::ZkIntegrityError,
        risc0::{groth16::inner::Inner, receipt_claim::compute_assert_claim, R0IntegrityVerifier},
        ZkPrecompile,
    },
};

pub struct R0Groth16Precompile;

impl ZkPrecompile for R0Groth16Precompile {
    fn verify_zk(dstack: &mut Stack) -> Result<(), ZkIntegrityError> {
        // The image id is a unique identifier of the ZK program,
        // any alterations to the program will change its image id.
        // Do note that the image id is not secret and can be known by anyone,
        // and that it is committed as part of the proof.
        let [image_id] = dstack.pop_raw()?;

        // The journal here is a digest of the public outputs of the ZK program.
        // Do note here that the R0 precompiles are verifying the executions of the
        // lift program, which verifies a previous ZK proof and outputs its public outputs as journal.
        // Rest assured, the integrity of the journal is still bound by the proof.
        let [journal] = dstack.pop_raw()?;

        // This contains the proof data itself.
        let [proof_data] = dstack.pop_raw()?;

        // Deserialize the proof and prepare for verification
        let inner: Inner = borsh::from_slice(&proof_data)?;

        // Convert image id and journal to Digests
        let image_id: Digest = Digest::try_from(image_id).map_err(ZkIntegrityError::Digest)?;
        let journal: Digest = Digest::try_from(journal).map_err(ZkIntegrityError::Digest)?;

        // This binds the claim of the proof, to the image id and the journal.
        compute_assert_claim(inner.claim(), image_id, journal).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;

        // Finally verify the integrity of the proof, which also asserts
        // that the bound claim is valid.
        inner.verify_integrity()
    }
}
