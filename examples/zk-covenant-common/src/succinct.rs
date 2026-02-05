use kaspa_txscript::opcodes::codes::{OpVerify, OpZkPrecompile};
use kaspa_txscript::script_builder::{ScriptBuilder, ScriptBuilderResult};

/// Converts a RISC0 hash function name string to its byte ID.
/// Returns None for unrecognized hash function names.
pub fn hashfn_str_to_id(s: &str) -> Option<u8> {
    match s {
        "blake2b" => Some(0),
        "poseidon2" => Some(1),
        "sha256" => Some(2),
        _ => None,
    }
}

pub trait Risc0SuccinctVerify {
    /// Verifies a RISC0 Succinct (STARK) proof.
    ///
    /// Expects on stack (bottom to top):
    ///   [seal, claim, hashfn, control_index, control_digests, journal_hash, image_id]
    ///
    /// Where:
    ///   - seal: STARK proof data (Vec<u32> as LE bytes)
    ///   - claim: Receipt claim digest (32 bytes)
    ///   - hashfn: Hash function ID (1 byte: 0=Blake2b, 1=Poseidon2, 2=Sha256)
    ///   - control_index: Merkle proof leaf index (u32 LE, 4 bytes)
    ///   - control_digests: Merkle proof path digests (N × 32 bytes)
    ///   - journal_hash: SHA256 hash of journal (32 bytes)
    ///   - image_id: Program ID (32 bytes)
    fn verify_risc0_succinct(&mut self) -> ScriptBuilderResult<&mut ScriptBuilder>;
}

impl Risc0SuccinctVerify for ScriptBuilder {
    fn verify_risc0_succinct(&mut self) -> ScriptBuilderResult<&mut ScriptBuilder> {
        // Stack: [seal, claim, hashfn, control_index, control_digests, journal_hash, image_id]
        self.add_data(&[0x21u8])?; // R0Succinct tag
                                   // Stack: [seal, claim, hashfn, control_index, control_digests, journal_hash, image_id, 0x21]
        self.add_op(OpZkPrecompile)?;
        // Stack: [true]
        self.add_op(OpVerify)
        // Stack: []
    }
}
