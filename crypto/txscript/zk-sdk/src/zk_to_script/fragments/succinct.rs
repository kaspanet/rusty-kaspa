use risc0_binfmt::Digestible;
use risc0_zkvm::SuccinctReceipt;

use super::prepare::prepare_r0_succinct_witness;
use crate::result::Result;
use kaspa_txscript::{
    opcodes::codes::OpZkPrecompile,
    script_builder::ScriptBuilder,
    zk_precompiles::{risc0::rcpt::HashFnId, tags::ZkTag},
};

/// Pushes the r0 succinct witness material into a caller-owned builder.
///
/// Pre-stack:  `[...]`
/// Post-stack: `[..., claim, control_index, control_digests, seal]`
/// the caller
/// must place `journal` on top afterwards, a constant push for a one-time
/// covenant, or a runtime in-script computation
pub fn push_r0_succinct_witness<Claim: Digestible + Clone>(
    builder: &mut ScriptBuilder,
    receipt: SuccinctReceipt<Claim>,
) -> Result<()> {
    let w = prepare_r0_succinct_witness(&receipt)?;
    builder.add_data(&w.claim)?;
    builder.add_data(&w.control_index)?;
    builder.add_data(&w.control_digests)?;
    builder.add_data(&w.seal)?;
    Ok(())
}

/// Appends the native r0 succinct verifier fragment into a caller-owned builder.
///
/// Pre-stack:  `[..., claim, control_index, control_digests, seal, journal]`
/// Post-stack: `[..., true]`
///
/// Embeds the image id, control id, hash function id (defaulting to Poseidon2),
/// the r0 succinct tag, and the r0 succinct zk precompile call.
pub fn append_r0_succinct_verifier(
    builder: &mut ScriptBuilder,
    image_id: [u8; 32],
    control_id: [u8; 32],
    hash_fn_id: Option<HashFnId>,
) -> Result<()> {
    // Image id: identifier of the program.
    builder.add_data(&image_id)?;
    // Control id: identifier of which r0 circuit was executed.
    builder.add_data(&control_id)?;
    // The hash function id is optional; defaults to Poseidon2 (the succinct default).
    builder.add_data([hash_fn_id.unwrap_or(HashFnId::Poseidon2) as u8].as_slice())?;
    // Tag this as an r0 succinct proof and dispatch the precompile.
    builder.add_data(&[ZkTag::R0Succinct as u8])?;
    builder.add_op(OpZkPrecompile)?;
    Ok(())
}

/// Appends a fixed-journal r0 succinct verifier fragment into a caller-owned
/// builder, binding the verification to a single `journal` baked into the script
///
/// Pre-stack:  `[..., claim, control_index, control_digests, seal]`
/// Post-stack:  `[..., true]`
///
pub fn append_r0_succinct_verifier_with_fixed_journal(
    builder: &mut ScriptBuilder,
    image_id: [u8; 32],
    control_id: [u8; 32],
    hash_fn_id: Option<HashFnId>,
    journal: [u8; 32],
) -> Result<()> {
    builder.add_data(&journal)?; // [..., claim, control_index, control_digests, seal, journal]
    append_r0_succinct_verifier(builder, image_id, control_id, hash_fn_id)
}
