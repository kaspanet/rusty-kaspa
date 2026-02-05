use crate::{
    data_stack::Stack,
    zk_precompiles::{
        ZkPrecompile,
        risc0::{
            merkle::MerkleProof,
            rcpt::{HashFnId, SuccinctReceipt},
            receipt_claim::compute_assert_claim,
        },
    },
};
use kaspa_txscript_errors::TxScriptError;
use risc0_zkp::core::digest::DIGEST_BYTES;
pub use risc0_zkp::core::digest::Digest;
mod error;
pub mod merkle;
pub mod rcpt;
pub mod receipt_claim;

pub struct R0SuccinctPrecompile;
pub use error::R0Error;

fn parse_digest(bytes: Vec<u8>) -> Result<Digest, R0Error> {
    Digest::try_from(bytes).map_err(R0Error::Digest)
}

fn parse_seal(bytes: Vec<u8>) -> Result<Vec<u32>, R0Error> {
    if bytes.len() % 4 != 0 {
        return Err(R0Error::InvalidSealLength(bytes.len()));
    }

    #[allow(clippy::incompatible_msrv, reason = "uses as_chunks from 1.88; ignore until we bump msrv on master")]
    Ok(bytes.as_chunks::<4>().0.iter().copied().map(u32::from_le_bytes).collect())
}

fn parse_hashfn(bytes: Vec<u8>) -> Result<HashFnId, R0Error> {
    if bytes.len() != 1 {
        return Err(R0Error::InvalidHashFnEncoding(bytes.len()));
    }

    HashFnId::try_from(bytes[0])
}

fn parse_merkle_index(bytes: Vec<u8>) -> Result<u32, R0Error> {
    if bytes.len() != 4 {
        return Err(R0Error::InvalidMerkleIndexLength(bytes.len()));
    }

    Ok(u32::from_le_bytes(bytes.as_slice().try_into().expect("index is 4 bytes")))
}

fn parse_digest_list(bytes: Vec<u8>) -> Result<Vec<Digest>, R0Error> {
    if bytes.len() % DIGEST_BYTES != 0 {
        return Err(R0Error::InvalidDigestLength(bytes.len()));
    }

    #[allow(clippy::incompatible_msrv, reason = "uses as_chunks from 1.88; ignore until we bump msrv on master")]
    Ok(bytes.as_chunks::<DIGEST_BYTES>().0.iter().copied().map(Digest::from_bytes).collect())
}

impl ZkPrecompile for R0SuccinctPrecompile {
    type Error = R0Error;
    /// Verifies the integrity of a RISC0 succinct proof receipt.
    ///
    /// *NOTE: Experimental code; not yet fully audited for mainnet use.* TODO(covpp-mainnet)
    ///
    /// Expects the following items on the stack (from top to bottom):
    /// - image id (bytes)
    /// - journal (bytes)
    /// - control inclusion proof digests (bytes)
    /// - control inclusion proof index (bytes, u32 le)
    /// - hash function id (bytes, u8)
    /// - claim (bytes)
    /// - seal (bytes, u32 le)
    fn verify_zk(dstack: &mut Stack) -> Result<(), Self::Error> {
        let [seal, claim, hashfn, control_index, control_digests, journal, image_id] = dstack.pop_raw()?;

        let seal = parse_seal(seal)?;
        let claim = parse_digest(claim)?;
        let hashfn = parse_hashfn(hashfn)?;
        let control_index = parse_merkle_index(control_index)?;
        let control_digests = parse_digest_list(control_digests)?;
        let control_inclusion_proof = MerkleProof { index: control_index, digests: control_digests };
        let rcpt = SuccinctReceipt::new(seal, claim, hashfn, control_inclusion_proof);

        // Convert image_id and journal to Digest, i.e. a hash
        let image_id: Digest = parse_digest(image_id)?;
        let journal: Digest = parse_digest(journal)?;

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
