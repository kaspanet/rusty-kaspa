use crate::result::Result;
use kaspa_txscript::zk_precompiles::risc0::rcpt::HashFnId;
use kaspa_txscript::{EngineFlags, script_builder::ScriptBuilder};
use risc0_binfmt::Digestible;
use risc0_zkvm::{Groth16Receipt, SuccinctReceipt};
use std::marker::PhantomData;
mod builder;
mod fragments;
#[cfg(any(feature = "wasm32-sdk", feature = "wasm32-core"))]
pub mod wasm;

pub use builder::{FinalizedR0Script, R0_SERIALIZED_UNCOMPRESSED_VK};
pub use fragments::{
    SuccinctWitnessBytes, append_r0_groth16_verifier, append_r0_groth16_verifier_with_fixed_journal, append_r0_succinct_verifier,
    append_r0_succinct_verifier_with_fixed_journal, prepare_r0_groth16_proof, prepare_r0_succinct_witness, push_r0_groth16_witness,
    push_r0_succinct_witness,
};

/// This represents R0 zk script builder
/// that has not yet been committed to a specific proof system
/// i.e. we have not yet added the tag nor which image id
/// or identifier we are proving for.
pub struct UnboundedR0Script;

/// An r0 script builder that committed to the
/// groth16 proof system, at this stage it can only accept
/// a groth16 proof in order to advance and finalize the script.
pub struct BoundedR0Groth16Script;

/// An r0 script builder that committed to the
/// succinct proof system, at this stage it can only accept
/// a succinct proof in order to advance and finalize the script.
pub struct BoundedR0SuccinctScript;

/// An r0 script builder that committed to the groth16 proof system *and* a
/// fixed journal hash baked into the script. It only needs the proof witness to
/// finalize — the journal is already constrained.
pub struct BoundedR0Groth16FixedJournalScript;

/// An r0 script builder that committed to the succinct proof system *and* a
/// fixed journal baked into the script. It only needs the receipt witness items
/// to finalize — the journal is already constrained.
pub struct BoundedR0SuccinctFixedJournalScript;

#[derive(Default)]
/// A wrapper around the native ScriptBuilder to abstract away the
/// complex implementation details of verifying a risc0 proof
/// whilst utilizing the OpZkPrecompile opcode.
pub struct R0ScriptBuilder<State> {
    builder: ScriptBuilder,
    _state: PhantomData<State>,
}

impl R0ScriptBuilder<UnboundedR0Script> {
    pub fn new() -> Self {
        Self { builder: ScriptBuilder::new(), _state: PhantomData }
    }

    pub fn with_flags(flags: EngineFlags) -> Self {
        Self { builder: ScriptBuilder::with_flags(flags), _state: PhantomData }
    }
}

impl<ScriptType> R0ScriptBuilder<ScriptType> {
    /// Get the script as bytes
    pub fn script(&self) -> &[u8] {
        self.builder.script()
    }

    /// Get a mutable reference to the script bytes,
    /// this allows for in place modifications
    pub fn script_mut(&mut self) -> &mut Vec<u8> {
        self.builder.script_mut()
    }

    /// Drain the builder and return the script as bytes,
    /// this consumes the builder
    pub fn drain(mut self) -> Vec<u8> {
        self.builder.drain()
    }

    /// Get a mutable reference to the inner `ScriptBuilder`. This allows the
    /// low-level fragment free functions to be applied to a staged builder
    /// regardless of which proof-system state it is in.
    pub fn builder_mut(&mut self) -> &mut ScriptBuilder {
        &mut self.builder
    }
}

impl<ScriptType> R0ScriptBuilder<ScriptType> {
    /// Pushes raw data (canonical encoding) — e.g. the caller-owned journal /
    /// journal_hash or a redeem script.
    pub fn add_data(&mut self, data: &[u8]) -> Result<&mut Self> {
        self.builder.add_data(data)?;
        Ok(self)
    }

    pub fn append_r0_groth16_verifier(&mut self, image_id: [u8; 32]) -> Result<&mut Self> {
        fragments::append_r0_groth16_verifier(&mut self.builder, image_id)?;
        Ok(self)
    }

    pub fn push_r0_groth16_witness<Claim: Digestible + Clone>(&mut self, receipt: Groth16Receipt<Claim>) -> Result<&mut Self> {
        fragments::push_r0_groth16_witness(&mut self.builder, receipt)?;
        Ok(self)
    }

    pub fn append_r0_groth16_verifier_with_fixed_journal(&mut self, image_id: [u8; 32], journal_hash: [u8; 32]) -> Result<&mut Self> {
        fragments::append_r0_groth16_verifier_with_fixed_journal(&mut self.builder, image_id, journal_hash)?;
        Ok(self)
    }

    pub fn push_r0_succinct_witness<Claim: Digestible + Clone>(&mut self, receipt: SuccinctReceipt<Claim>) -> Result<&mut Self> {
        fragments::push_r0_succinct_witness(&mut self.builder, receipt)?;
        Ok(self)
    }

    pub fn append_r0_succinct_verifier(
        &mut self,
        image_id: [u8; 32],
        control_id: [u8; 32],
        hash_fn_id: Option<HashFnId>,
    ) -> Result<&mut Self> {
        fragments::append_r0_succinct_verifier(&mut self.builder, image_id, control_id, hash_fn_id)?;
        Ok(self)
    }

    pub fn append_r0_succinct_verifier_with_fixed_journal(
        &mut self,
        image_id: [u8; 32],
        control_id: [u8; 32],
        hash_fn_id: Option<HashFnId>,
        journal: [u8; 32],
    ) -> Result<&mut Self> {
        fragments::append_r0_succinct_verifier_with_fixed_journal(&mut self.builder, image_id, control_id, hash_fn_id, journal)?;
        Ok(self)
    }
}

impl From<ScriptBuilder> for R0ScriptBuilder<UnboundedR0Script> {
    fn from(value: ScriptBuilder) -> Self {
        R0ScriptBuilder { builder: value, _state: PhantomData }
    }
}
