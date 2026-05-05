use std::marker::PhantomData;

use crate::{opcodes::codes::OpZkPrecompile, script_builder::ScriptBuilder};
use super::result::Result;
pub mod groth16;
//mod succinct;
mod builder;

struct UninitializedZkScript;
struct UnboundedZkScript; 
struct BoundedGroth16Script;
struct BoundedR0SuccinctScript;
struct FinalizedZkScript;
#[non_exhaustive]
pub struct R0ScriptBuilder<State> {
    builder: ScriptBuilder,
    _state:PhantomData<State>,
}

impl R0ScriptBuilder<UninitializedZkScript> {
    pub fn new() -> Self {
        Self {
            builder: ScriptBuilder::new(),
            _state: PhantomData,
        }
    }

    /// Initializes that this script is a zk precompile script
    /// by adding the OpZkPrecompile opcode.
    pub fn initialize( self) -> Result<R0ScriptBuilder<UnboundedZkScript>> {
        Ok(R0ScriptBuilder {
            builder: self.builder,
            _state: PhantomData,
        })
    }
}

impl From<ScriptBuilder> for R0ScriptBuilder<UninitializedZkScript> {
    fn from(value: ScriptBuilder) -> Self {
        R0ScriptBuilder {
            builder: value,
            _state: PhantomData,
        }
    }
}