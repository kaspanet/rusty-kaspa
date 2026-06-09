mod commit;
mod proof;

use crate::error::Error;
use crate::result::Result;
use crate::wasm::builder::ScriptBuilderOptions;
use crate::zk_precompiles::risc0::zk_to_script::{
    BoundedR0Groth16Script, BoundedR0SuccinctScript, R0ScriptBuilder as NativeR0ScriptBuilder, UnboundedR0Script,
};
use crate::EngineFlags;
use kaspa_wasm_core::types::HexString;
use wasm_bindgen::prelude::wasm_bindgen;

/// Runtime mirror of the native compile-time type-state. `Taken` is a
/// transient sentinel held while ownership of the inner native builder is
/// being moved across a state transition.
pub(super) enum InnerState {
    Unbounded(NativeR0ScriptBuilder<UnboundedR0Script>),
    BoundedGroth16(NativeR0ScriptBuilder<BoundedR0Groth16Script>),
    BoundedSuccinct(NativeR0ScriptBuilder<BoundedR0SuccinctScript>),
    Taken,
}

impl InnerState {
    fn script(&self) -> &[u8] {
        match self {
            InnerState::Unbounded(b) => b.script(),
            InnerState::BoundedGroth16(b) => b.script(),
            InnerState::BoundedSuccinct(b) => b.script(),
            InnerState::Taken => &[],
        }
    }

    fn drain(&mut self) -> Vec<u8> {
        match std::mem::replace(self, InnerState::Taken) {
            InnerState::Unbounded(b) => b.drain(),
            InnerState::BoundedGroth16(b) => b.drain(),
            InnerState::BoundedSuccinct(b) => b.drain(),
            InnerState::Taken => Vec::new(),
        }
    }
}

pub(super) fn into_array_32(bytes: Vec<u8>, name: &'static str) -> Result<[u8; 32]> {
    bytes.as_slice().try_into().map_err(|_| Error::custom(format!("{name} must be 32 bytes")))
}

/// R0ScriptBuilder provides a staged builder for RISC0 zk-to-script locking
/// scripts. It enforces its state machine at runtime, since WASM cannot
/// express the native compile-time type-state transitions.
///
/// Flow:
///   1. `new()` — unbounded.
///   2. `commitToGroth16(imageId)` *or* `commitToSuccinct(imageId, controlId, hashFnId?)` — bounded.
///   3. `finalizeWithGroth16Proof(receipt, journalHash)` *or* `finalizeWithSuccinctProof(receipt, journal)` — finalized hex bytes.
///
/// Calling a method in the wrong state returns an error.
#[wasm_bindgen(inspectable)]
pub struct R0ScriptBuilder {
    pub(super) inner: InnerState,
}

impl Default for R0ScriptBuilder {
    fn default() -> Self {
        Self { inner: InnerState::Unbounded(NativeR0ScriptBuilder::new()) }
    }
}

impl R0ScriptBuilder {
    pub(super) fn take(&mut self) -> InnerState {
        std::mem::replace(&mut self.inner, InnerState::Taken)
    }
}

#[wasm_bindgen]
impl R0ScriptBuilder {
    /// Constructs a new R0ScriptBuilder. Accepts an optional
    /// `ScriptBuilderOptions` object
    /// whose `flags` are forwarded to the underlying native builder. When
    /// omitted, the native default `EngineFlags` are used.
    #[wasm_bindgen(constructor)]
    pub fn new(options: Option<ScriptBuilderOptions>) -> Result<R0ScriptBuilder> {
        let flags = options.map(EngineFlags::try_from).transpose()?.unwrap_or_default();
        Ok(Self { inner: InnerState::Unbounded(NativeR0ScriptBuilder::with_flags(flags)) })
    }

    /// Drains (empties) the builder and returns the script bytes as a hex
    /// string.
    pub fn drain(&mut self) -> HexString {
        HexString::from(self.inner.drain().as_slice())
    }

    /// Returns the current script bytes as a hex string.
    pub fn script(&self) -> HexString {
        HexString::from(self.inner.script())
    }
}
