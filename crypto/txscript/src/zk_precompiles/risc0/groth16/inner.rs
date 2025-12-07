use alloc::vec::Vec;

use borsh::{BorshDeserialize, BorshSerialize};
use risc0_circuit_recursion::control_id::{ALLOWED_CONTROL_ROOT, BN254_IDENTITY_CONTROL_ID};
use risc0_groth16::Verifier;
use risc0_zkp::{core::digest::Digest, verify::VerificationError};
use serde::{Deserialize, Serialize};

use crate::zk_precompiles::{error::ZkIntegrityError, risc0::R0IntegrityVerifier};

/// A receipt composed of a Groth16 over the BN_254 curve.
/// This struct is a modified version of the Groth16Receipt defined in
/// risc0. The reason for this is to simplify it, as we are certain to only receive digests
/// for the claim and verifier parameters.
#[derive(Clone, Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Inner {
    /// A Groth16 proof of a zkVM execution with the associated claim.
    seal: Vec<u8>,

    /// [ReceiptClaim][crate::ReceiptClaim] containing information about the execution that this
    /// receipt proves.
    claim: Digest,

    /// A digest of the verifier parameters that can be used to verify this receipt.
    ///
    /// Acts as a fingerprint to identify differing proof system or circuit versions between a
    /// prover and a verifier. Is not intended to contain the full verifier parameters, which must
    /// be provided by a trusted source (e.g. packaged with the verifier code).
    verifier_parameters: Digest,
}

impl Inner {
    pub fn claim(&self) -> &Digest {
        &self.claim
    }
}

impl R0IntegrityVerifier for Inner {
    /// Verify the integrity of this receipt, ensuring the claim is attested
    /// to by the seal.
    fn verify_integrity(&self) -> Result<(), ZkIntegrityError> {
        Verifier::new(&self.seal, ALLOWED_CONTROL_ROOT, self.claim, BN254_IDENTITY_CONTROL_ID, &risc0_groth16::verifying_key())
            .map_err(|_| VerificationError::ReceiptFormatError)?
            .verify()
            .map_err(|_| VerificationError::InvalidProof)?;

        // Everything passed
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use risc0_binfmt::tagged_struct;
    use risc0_binfmt::Digestible;
    use risc0_circuit_recursion::control_id::{ALLOWED_CONTROL_ROOT, BN254_IDENTITY_CONTROL_ID};
    use risc0_zkp::core::{digest::digest, hash::sha::Impl};

    // Check that the verifier parameters has a stable digest (and therefore a stable value). This
    // struct encodes parameters used in verification, and so this value should be updated if and
    // only if a change to the verifier parameters is expected. Updating the verifier parameters
    // will result in incompatibility with previous versions.
    #[test]
    fn groth16_receipt_verifier_parameters_is_stable() {
        assert_eq!(
            tagged_struct::<Impl>(
                "risc0.Groth16ReceiptVerifierParameters",
                &[ALLOWED_CONTROL_ROOT, BN254_IDENTITY_CONTROL_ID, risc0_groth16::verifying_key().digest::<Impl>(),],
                &[],
            ),
            digest!("73c457ba541936f0d907daf0c7253a39a9c5c427c225ba7709e44702d3c6eedc")
        );
    }
}
