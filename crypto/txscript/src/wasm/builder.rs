use crate::{EngineFlags, error::Error, result::Result, script_builder as native, standard};
use kaspa_consensus_core::{mass::ScriptUnits, tx::ScriptPublicKey};
use kaspa_utils::hex::ToHex;
use kaspa_wasm_core::hex::{HexViewConfig, HexViewConfigT};
use kaspa_wasm_core::types::{BinaryT, HexString};
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;
use wasm_bindgen::prelude::wasm_bindgen;
use workflow_wasm::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_SCRIPT_BUILDER_OPTIONS: &'static str = r#"
/**
 * Script builder engine flags.
 *
 * @category TxScript
 */
export interface ScriptBuilderFlags {
    /** Whether or not covenant opcodes and post-Toccata script limits are enabled. */
    covenantsEnabled?: boolean;
    /** Script units charged for each signature operation. Defaults to the native engine default. */
    sigopScriptUnits?: bigint | number;
}

/**
 * Script builder options.
 *
 * @category TxScript
 */
export interface ScriptBuilderOptions {
    /** Engine flags used by the underlying native ScriptBuilder. */
    flags?: ScriptBuilderFlags;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "ScriptBuilderOptions")]
    pub type ScriptBuilderOptions;
}

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
    fn with_flags(flags: EngineFlags) -> Self {
        Self { script_builder: Rc::new(RefCell::new(native::ScriptBuilder::with_flags(flags))) }
    }

    fn try_new(options: Option<ScriptBuilderOptions>) -> Result<Self> {
        let flags = options.map(EngineFlags::try_from).transpose()?.unwrap_or_default();
        Ok(Self::with_flags(flags))
    }

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
        Self::with_flags(Default::default())
    }
}

impl TryFrom<ScriptBuilderOptions> for EngineFlags {
    type Error = Error;

    fn try_from(value: ScriptBuilderOptions) -> Result<Self> {
        let object = js_sys::Object::try_from(&value).ok_or_else(|| Error::Custom("options must be an object".into()))?;
        let flags = object.try_get_value("flags")?;
        let Some(flags) = flags else {
            return Ok(Self::default());
        };

        let flags = js_sys::Object::try_from(&flags).ok_or_else(|| Error::Custom("options.flags must be an object".into()))?;
        let mut engine_flags = Self::default();

        if let Some(value) = flags.try_get_value("covenantsEnabled")? {
            engine_flags.covenants_enabled =
                value.as_bool().ok_or_else(|| Error::convert("flags.covenantsEnabled", "expected boolean"))?;
        }

        if flags.try_get_value("sigopScriptUnits")?.is_some() {
            engine_flags.sigop_script_units =
                ScriptUnits(flags.get_u64("sigopScriptUnits").map_err(|err| Error::convert("flags.sigopScriptUnits", err))?);
        }

        Ok(engine_flags)
    }
}

#[wasm_bindgen]
impl ScriptBuilder {
    #[wasm_bindgen(constructor)]
    pub fn new(options: Option<ScriptBuilderOptions>) -> Result<ScriptBuilder> {
        Self::try_new(options)
    }

    /// Creates a new ScriptBuilder over an existing script.
    /// Supplied script can be represented as an `Uint8Array` or a `HexString`.
    #[wasm_bindgen(js_name = "fromScript")]
    pub fn from_script(script: BinaryT, options: Option<ScriptBuilderOptions>) -> Result<ScriptBuilder> {
        let builder = ScriptBuilder::try_new(options)?;
        let script = script.try_as_vec_u8()?;
        builder.inner_mut().script_mut().extend(&script);

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
    /// and any push of data greater than the maximum script element size will not modify
    /// the script since that is not allowed by the script engine.
    ///
    /// Also, the script will not be modified if pushing the data would cause the script to
    /// exceed the maximum allowed script engine size.
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
        let flags = inner.flags();
        let signature = signature.try_as_vec_u8()?;
        let generated_script = standard::pay_to_script_hash_signature_script_with_flags(script.into(), signature, flags)?;

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

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::{ScriptBuilder, ScriptBuilderOptions};
    use crate::{max_script_element_size, wasm::Opcodes};
    use js_sys::{Object, Reflect, Uint8Array};
    use kaspa_wasm_core::types::BinaryT;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_test::wasm_bindgen_test;

    fn binary(bytes: &[u8]) -> BinaryT {
        Uint8Array::from(bytes).unchecked_into()
    }

    fn script_builder_options(covenants_enabled: bool) -> ScriptBuilderOptions {
        let flags = Object::new();
        Reflect::set(&flags, &JsValue::from_str("covenantsEnabled"), &JsValue::from_bool(covenants_enabled)).unwrap();

        let options = Object::new();
        Reflect::set(&options, &JsValue::from_str("flags"), &flags).unwrap();
        options.unchecked_into()
    }

    #[wasm_bindgen_test]
    fn script_builder_js_test() {
        let builder = ScriptBuilder::new(None).expect("builder should be created");
        builder.add_op(Opcodes::OpTrue as u8).expect("opcode should be added");
        builder.add_data(binary(&[0xab, 0xcd])).expect("data should be added");

        assert_eq!(builder.to_string_js().as_string().unwrap(), "5102abcd");
    }

    #[wasm_bindgen_test]
    fn script_builder_uses_flags_js_test() {
        let data_with_length_greater_than_max = vec![0x01; max_script_element_size(false) + 1];

        let builder_without_covenants = ScriptBuilder::new(None).expect("builder should be created");
        assert!(builder_without_covenants.add_data(binary(&data_with_length_greater_than_max)).is_err());

        let builder_with_covenants =
            ScriptBuilder::new(Some(script_builder_options(true))).expect("builder should be created with covenant flags");
        builder_with_covenants.add_data(binary(&data_with_length_greater_than_max)).expect("covenant flag allow pushes > 520");
    }
}
