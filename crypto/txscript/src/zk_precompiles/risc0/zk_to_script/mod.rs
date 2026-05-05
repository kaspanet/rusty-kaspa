use super::result::Result;
use crate::script_builder::ScriptBuilder;
use std::marker::PhantomData;
mod builder;
#[cfg(any(feature = "wasm32-sdk", feature = "wasm32-core"))]
pub mod wasm;

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

/// A finalized r0 script builder, at this stage the script is finalized
/// no further changes can be done.
pub struct FinalizedR0Script;

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
}

impl From<ScriptBuilder> for R0ScriptBuilder<UnboundedR0Script> {
    fn from(value: ScriptBuilder) -> Self {
        R0ScriptBuilder { builder: value, _state: PhantomData }
    }
}
