use crate::imports::*;
use crate::result::Result;
use kaspa_txscript::script_builder::ScriptBuilder as Inner;

///
///  ScriptBuilder provides a facility for building custom scripts. It allows
/// you to push opcodes, ints, and data while respecting canonical encoding. In
/// general it does not ensure the script will execute correctly, however any
/// data pushes which would exceed the maximum allowed script engine limits and
/// are therefore guaranteed not to execute will not be pushed and will result in
/// the Script function returning an error.
///
#[derive(Clone)]
#[wasm_bindgen]
pub struct ScriptBuilder {
    inner: Arc<Mutex<Inner>>,
}

impl ScriptBuilder {
    pub fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap()
    }
}

#[wasm_bindgen]
impl ScriptBuilder {
    /// Get script bytes represented by a hex string.
    pub fn script(&self) -> String {
        let inner = self.inner();
        let script = inner.script();
        script.to_hex()
    }

    /// Drains (empties) the script builder, returning the
    /// script bytes represented by a hex string.
    pub fn drain(&self) -> String {
        let mut inner = self.inner();
        let script = inner.drain();
        script.to_hex()
    }

    /// Pushes the passed opcode to the end of the script. The script will not
    /// be modified if pushing the opcode would cause the script to exceed the
    /// maximum allowed script engine size.
    #[wasm_bindgen(js_name = "addOp")]
    pub fn add_op(&self, opcode: u8) -> Result<ScriptBuilder> {
        self.inner().add_op(opcode)?;
        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = "addOps")]
    pub fn add_ops(&self, opcodes: JsValue) -> Result<ScriptBuilder> {
        let opcodes = opcodes.try_as_vec_u8()?;
        self.inner().add_ops(&opcodes)?;
        Ok(self.clone())
    }

    /// AddData pushes the passed data to the end of the script. It automatically
    /// chooses canonical opcodes depending on the length of the data.
    ///
    /// A zero length buffer will lead to a push of empty data onto the stack (Op0 = OpFalse)
    /// and any push of data greater than [`MAX_SCRIPT_ELEMENT_SIZE`] will not modify
    /// the script since that is not allowed by the script engine.
    ///
    /// Also, the script will not be modified if pushing the data would cause the script to
    /// exceed the maximum allowed script engine size [`MAX_SCRIPTS_SIZE`].
    #[wasm_bindgen(js_name = "addData")]
    pub fn add_data(&self, data: JsValue) -> Result<ScriptBuilder> {
        let data = data.try_as_vec_u8()?;
        self.inner().add_data(&data)?;
        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = "addI64")]
    pub fn add_i64(&self, val: i64) -> Result<ScriptBuilder> {
        self.inner().add_i64(val)?;
        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = "addLockTime")]
    pub fn add_lock_time(&self, lock_time: u64) -> Result<ScriptBuilder> {
        self.inner().add_lock_time(lock_time)?;
        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = "addSequence")]
    pub fn add_sequence(&self, sequence: u64) -> Result<ScriptBuilder> {
        self.inner().add_sequence(sequence)?;
        Ok(self.clone())
    }
}
