use crate::{
    data_stack::Stack,
    runtime_resource_meter::RuntimeResourceMeter,
    zk_precompiles::{
        ZkPrecompile,
        risc0::{
            merkle::MerkleProof,
            rcpt::{HashFnId, SuccinctReceipt},
            receipt_claim::compute_assert_claim,
        },
    },
};
use risc0_zkp::core::digest::DIGEST_BYTES;
pub use risc0_zkp::core::digest::Digest;
mod error;
pub mod merkle;
pub mod rcpt;
pub mod receipt_claim;

pub struct R0SuccinctPrecompile;
pub use error::R0Error;

// Matches risc0-zkvm's ALLOWED_CODE_MERKLE_DEPTH for the Poseidon2 control root.
const POSEIDON2_CONTROL_MERKLE_DEPTH: usize = 8;

/// Returns the control Merkle tree depth for the supported RISC0 hash function.
fn control_merkle_depth_for(hashfn: HashFnId) -> usize {
    match hashfn {
        HashFnId::Poseidon2 => POSEIDON2_CONTROL_MERKLE_DEPTH,
        HashFnId::Blake2b | HashFnId::Sha256 => unreachable!("unsupported hashfn was checked above"),
    }
}

fn parse_digest(bytes: impl AsRef<[u8]>) -> Result<Digest, R0Error> {
    let bytes = bytes.as_ref();
    Digest::try_from(bytes).map_err(|_| R0Error::InvalidDigestLength(bytes.len()))
}

fn parse_seal(bytes: impl AsRef<[u8]>) -> Result<Vec<u32>, R0Error> {
    let bytes = bytes.as_ref();
    let (chunks, remaining) = bytes.as_chunks::<4>();
    if !remaining.is_empty() {
        // we require no remainder
        Err(R0Error::InvalidSealLength(bytes.len()))
    } else {
        Ok(chunks.iter().copied().map(u32::from_le_bytes).collect())
    }
}

fn parse_hashfn(bytes: impl AsRef<[u8]>) -> Result<HashFnId, R0Error> {
    let bytes = bytes.as_ref();
    if bytes.len() != 1 {
        return Err(R0Error::InvalidHashFnEncoding(bytes.len()));
    }

    HashFnId::try_from(bytes[0])
}

fn parse_merkle_index(bytes: impl AsRef<[u8]>) -> Result<u32, R0Error> {
    let bytes = bytes.as_ref();
    if bytes.len() != 4 {
        return Err(R0Error::InvalidMerkleIndexLength(bytes.len()));
    }

    Ok(u32::from_le_bytes(bytes.try_into().expect("index is 4 bytes")))
}

fn parse_digest_list(bytes: impl AsRef<[u8]>) -> Result<Vec<Digest>, R0Error> {
    let bytes = bytes.as_ref();
    let (chunks, remaining) = bytes.as_chunks::<DIGEST_BYTES>();
    if !remaining.is_empty() {
        // we require no remainder
        Err(R0Error::InvalidDigestListLength(bytes.len()))
    } else {
        Ok(chunks.iter().copied().map(Digest::from).collect())
    }
}

impl ZkPrecompile for R0SuccinctPrecompile {
    type Error = R0Error;
    /// Verifies the integrity of a RISC0 succinct proof receipt.
    ///
    /// Expects the following items on the stack (from top to bottom):
    /// - hash function id (bytes, u8)
    /// - control id (bytes, digest length)
    /// - image id (bytes, digest length)
    /// - journal (bytes, digest length)
    /// - seal (bytes, list of u32 le)
    /// - control inclusion proof digests (bytes)
    /// - control index (bytes, u32 le)
    /// - claim (bytes)
    fn verify_zk(dstack: &mut Stack, _meter: &mut RuntimeResourceMeter) -> Result<(), Self::Error> {
        let [claim, control_index, control_digests, seal, journal, image_id, control_id, hashfn] = dstack.pop_raw()?;

        let control_id = parse_digest(control_id)?;
        let seal = parse_seal(seal)?;
        let claim = parse_digest(claim)?;
        let hashfn = parse_hashfn(hashfn)?;

        // For now we only support the poseidon2 hashfn.
        // See usage of poseidon2's ALLOWED_CONTROL_ROOT within verify_integrity::check_code
        if hashfn != HashFnId::Poseidon2 {
            return Err(R0Error::UnsupportedHashFn(hashfn));
        }

        let control_index = parse_merkle_index(control_index)?;
        let control_digests = parse_digest_list(control_digests)?;

        let max_control_proof_len = control_merkle_depth_for(hashfn);
        if control_digests.len() > max_control_proof_len {
            return Err(R0Error::ControlInclusionProofTooLong { actual: control_digests.len(), max: max_control_proof_len });
        }

        let control_inclusion_proof = MerkleProof { index: control_index, digests: control_digests };
        let rcpt = SuccinctReceipt::new(seal, control_id, claim, hashfn, control_inclusion_proof);

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
        compute_assert_claim(rcpt.claim(), image_id, journal)?;

        Ok(())
    }
}
