use crate::result::Result;
use crate::{script_builder as native, standard};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_utils::hex::ToHex;
use kaspa_wasm_core::hex::{HexViewConfig, HexViewConfigT};
use kaspa_wasm_core::types::{BinaryT, HexString};
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

/// ScriptBuilder provides a facility for building custom scripts. It allows
/// you to push opcodes, ints, and data while respecting canonical encoding. In
/// general it does not ensure the script will execute correctly, however any
/// data pushes which would exceed the maximum allowed script engine limits and
/// are therefore guaranteed not to execute will not be pushed and will result in
/// the Script function returning an error.
/// @category Consensus
#[derive(Clone)]
#[wasm_bindgen(inspectable)]
pub struct ScriptBuilder {
    script_builder: Rc<RefCell<native::ScriptBuilder>>,
}

impl ScriptBuilder {
    #[inline]
    pub fn inner(&self) -> Ref<'_, native::ScriptBuilder> {
        self.script_builder.borrow()
    }

    #[inline]
    pub fn inner_mut(&self) -> RefMut<'_, native::ScriptBuilder> {
        self.script_builder.borrow_mut()
    }
}

impl Default for ScriptBuilder {
    fn default() -> Self {
        Self { script_builder: Rc::new(RefCell::new(native::ScriptBuilder::new())) }
    }
}

#[wasm_bindgen]
impl ScriptBuilder {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new ScriptBuilder over an existing script.
    /// Supplied script can be represented as an `Uint8Array` or a `HexString`.
    #[wasm_bindgen(js_name = "fromScript")]
    pub fn from_script(script: BinaryT) -> Result<ScriptBuilder> {
        let builder = ScriptBuilder::default();
        let script = script.try_as_vec_u8()?;
        builder.inner_mut().extend(&script);

        Ok(builder)
    }

    /// Pushes the passed opcode to the end of the script. The script will not
    /// be modified if pushing the opcode would cause the script to exceed the
    /// maximum allowed script engine size.
    #[wasm_bindgen(js_name = "addOp")]
    pub fn add_op(&self, op: u8) -> Result<ScriptBuilder> {
        let mut inner = self.inner_mut();
        inner.add_op(op)?;

        Ok(self.clone())
    }

    /// Adds the passed opcodes to the end of the script.
    /// Supplied opcodes can be represented as an `Uint8Array` or a `HexString`.
    #[wasm_bindgen(js_name = "addOps")]
    pub fn add_ops(&self, opcodes: BinaryT) -> Result<ScriptBuilder> {
        let opcodes = opcodes.try_as_vec_u8()?;
        self.inner_mut().add_ops(&opcodes)?;

        Ok(self.clone())
    }

    /// AddData pushes the passed data to the end of the script. It automatically
    /// chooses canonical opcodes depending on the length of the data.
    ///
    /// A zero length buffer will lead to a push of empty data onto the stack (Op0 = OpFalse)
    /// and any push of data greater than [`MAX_SCRIPT_ELEMENT_SIZE`](kaspa_txscript::MAX_SCRIPT_ELEMENT_SIZE) will not modify
    /// the script since that is not allowed by the script engine.
    ///
    /// Also, the script will not be modified if pushing the data would cause the script to
    /// exceed the maximum allowed script engine size [`MAX_SCRIPTS_SIZE`](kaspa_txscript::MAX_SCRIPTS_SIZE).
    #[wasm_bindgen(js_name = "addData")]
    pub fn add_data(&self, data: BinaryT) -> Result<ScriptBuilder> {
        let data = data.try_as_vec_u8()?;

        let mut inner = self.inner_mut();
        inner.add_data(&data)?;

        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = "addI64")]
    pub fn add_i64(&self, value: i64) -> Result<ScriptBuilder> {
        let mut inner = self.inner_mut();
        inner.add_i64(value)?;

        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = "addLockTime")]
    pub fn add_lock_time(&self, lock_time: u64) -> Result<ScriptBuilder> {
        let mut inner = self.inner_mut();
        inner.add_lock_time(lock_time)?;

        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = "addSequence")]
    pub fn add_sequence(&self, sequence: u64) -> Result<ScriptBuilder> {
        let mut inner = self.inner_mut();
        inner.add_sequence(sequence)?;

        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = "canonicalDataSize")]
    pub fn canonical_data_size(data: BinaryT) -> Result<u32> {
        let data = data.try_as_vec_u8()?;
        let size = native::ScriptBuilder::canonical_data_size(&data) as u32;

        Ok(size)
    }

    /// Get script bytes represented by a hex string.
    #[wasm_bindgen(js_name = "toString")]
    pub fn to_string_js(&self) -> HexString {
        let inner = self.inner();

        HexString::from(inner.script())
    }

    /// Drains (empties) the script builder, returning the
    /// script bytes represented by a hex string.
    pub fn drain(&self) -> HexString {
        let mut inner = self.inner_mut();

        HexString::from(inner.drain().as_slice())
    }

    /// Creates an equivalent pay-to-script-hash script.
    /// Can be used to create an P2SH address.
    /// @see {@link addressFromScriptPublicKey}
    #[wasm_bindgen(js_name = "createPayToScriptHashScript")]
    pub fn pay_to_script_hash_script(&self) -> ScriptPublicKey {
        let inner = self.inner();
        let script = inner.script();

        standard::pay_to_script_hash_script(script)
    }

    /// Generates a signature script that fits a pay-to-script-hash script.
    #[wasm_bindgen(js_name = "encodePayToScriptHashSignatureScript")]
    pub fn pay_to_script_hash_signature_script(&self, signature: BinaryT) -> Result<HexString> {
        let inner = self.inner();
        let script = inner.script();
        let signature = signature.try_as_vec_u8()?;
        let generated_script = standard::pay_to_script_hash_signature_script(script.into(), signature)?;

        Ok(generated_script.to_hex().into())
    }

    #[wasm_bindgen(js_name = "hexView")]
    pub fn hex_view(&self, args: Option<HexViewConfigT>) -> Result<String> {
        let inner = self.inner();
        let script = inner.script();

        let config = args.map(HexViewConfig::try_from).transpose()?.unwrap_or_default();
        Ok(config.build(script).to_string())
    }
}
