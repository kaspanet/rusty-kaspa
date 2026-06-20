pub mod error;
mod points;
pub mod result;
pub mod zk_to_script;

pub use zk_to_script::{
    BoundedR0Groth16FixedJournalScript, BoundedR0Groth16Script, BoundedR0SuccinctFixedJournalScript, BoundedR0SuccinctScript,
    FinalizedR0Script, R0_SERIALIZED_UNCOMPRESSED_VK, SuccinctWitnessBytes, UnboundedR0Script, ZkScriptBuilder,
    append_r0_groth16_verifier, append_r0_groth16_verifier_with_fixed_journal, append_r0_succinct_verifier,
    append_r0_succinct_verifier_with_fixed_journal, prepare_r0_groth16_proof, prepare_r0_succinct_witness, push_r0_groth16_witness,
    push_r0_succinct_witness,
};

#[cfg(any(feature = "wasm32-sdk", feature = "wasm32-core"))]
pub use zk_to_script::wasm;
