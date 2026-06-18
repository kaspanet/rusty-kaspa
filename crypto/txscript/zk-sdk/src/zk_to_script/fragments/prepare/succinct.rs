use crate::result::Result;
use risc0_binfmt::Digestible;
use risc0_zkvm::{SuccinctReceipt, sha};

/// The receipt-derived witness items for a RISC0 succinct proof, in the order
/// the verifier fragment consumes them.
///
/// The journal is intentionally **excluded** — it is caller-owned (pushed as a
/// constant for a fixed-journal covenant, or computed at runtime in-script for
/// the general case).
pub struct SuccinctWitnessBytes {
    /// The SHA-256 digest of the receipt claim (32 bytes).
    pub claim: Vec<u8>,
    /// The control inclusion-proof leaf index (`u32` little-endian, 4 bytes).
    pub control_index: Vec<u8>,
    /// The flattened control inclusion-proof sibling digests.
    pub control_digests: Vec<u8>,
    /// The flattened STARK seal (`u32` little-endian words).
    pub seal: Vec<u8>,
}

/// Maps a RISC0 succinct receipt to its four on-stack witness byte vectors.
///
/// Pure transformation with no [`ScriptBuilder`] involvement: a transaction
/// builder can compute the bytes once and push them however it likes.
/// [`push_r0_succinct_witness`] is implemented on top of this function.
///
/// [`ScriptBuilder`]: kaspa_txscript::script_builder::ScriptBuilder
/// [`push_r0_succinct_witness`]: super::super::push_r0_succinct_witness
pub fn prepare_r0_succinct_witness<Claim: Digestible + Clone>(receipt: &SuccinctReceipt<Claim>) -> Result<SuccinctWitnessBytes> {
    // The claim might already be digested or not; in either case we extract the
    // digest, since that is what keeps the stark proof constant-sized.
    let claim = receipt.claim.digest::<sha::Impl>();

    // The control index and control digests form the merkle proof of inclusion.
    let control_index = receipt.control_inclusion_proof.index.to_le_bytes().to_vec();
    let control_digests = receipt.control_inclusion_proof.digests.iter().flat_map(|d| d.as_bytes().to_vec()).collect::<Vec<u8>>();

    // Encode the seal as a flat vec<u8> of u32 little-endian words.
    let seal = receipt.seal.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();

    Ok(SuccinctWitnessBytes { claim: claim.as_bytes().to_vec(), control_index, control_digests, seal })
}
