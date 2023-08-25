use crate::imports::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionOutputInner {
    pub value: u64,
    #[wasm_bindgen(js_name = scriptPublicKey, getter_with_clone)]
    pub script_public_key: ScriptPublicKey,
}

/// Represents a Kaspad transaction output
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionOutput {
    inner: Arc<Mutex<TransactionOutputInner>>,
}

impl TransactionOutput {
    pub fn new_with_inner(inner: TransactionOutputInner) -> Self {
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn inner(&self) -> MutexGuard<'_, TransactionOutputInner> {
        self.inner.lock().unwrap()
    }

    pub fn script_length(&self) -> usize {
        self.inner().script_public_key.script().len()
    }

    // pub fn set_amount(&self, amount: u64) {
    //     self.inner().value = amount;
    // }
    // pub fn value(&self) -> u64 {
    //     self.inner().value
    // }
}

#[wasm_bindgen]
impl TransactionOutput {
    #[wasm_bindgen(constructor)]
    /// TransactionOutput constructor
    pub fn new(value: u64, script_public_key: &ScriptPublicKey) -> TransactionOutput {
        Self { inner: Arc::new(Mutex::new(TransactionOutputInner { value, script_public_key: script_public_key.clone() })) }
    }

    #[wasm_bindgen(getter, js_name = value)]
    pub fn get_value(&self) -> u64 {
        self.inner().value
    }

    #[wasm_bindgen(setter, js_name = value)]
    pub fn set_value(&self, v: u64) {
        self.inner().value = v;
    }

    #[wasm_bindgen(getter, js_name = scriptPublicKey)]
    pub fn get_script_public_key(&self) -> ScriptPublicKey {
        self.inner().script_public_key.clone()
    }

    #[wasm_bindgen(setter, js_name = scriptPublicKey)]
    pub fn set_script_public_key(&self, v: &ScriptPublicKey) {
        self.inner().script_public_key = v.clone();
    }

    // TODO-WASM
    // #[wasm_bindgen(js_name=isDust)]
    // pub fn is_dust(&self) -> bool {
    //     is_transaction_output_dust(self)
    // }
}

impl AsRef<TransactionOutput> for TransactionOutput {
    fn as_ref(&self) -> &TransactionOutput {
        self
    }
}

impl From<cctx::TransactionOutput> for TransactionOutput {
    fn from(output: cctx::TransactionOutput) -> Self {
        TransactionOutput::new(output.value, &output.script_public_key)
    }
}

impl From<&TransactionOutput> for cctx::TransactionOutput {
    fn from(output: &TransactionOutput) -> Self {
        let inner = output.inner();
        cctx::TransactionOutput::new(inner.value, inner.script_public_key.clone())
    }
}

impl TryFrom<JsValue> for TransactionOutput {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        // workflow_log::log_trace!("js_value->TransactionOutput: {js_value:?}");
        if let Some(object) = Object::try_from(&js_value) {
            let has_address = Object::has_own(object, &JsValue::from("address"));
            workflow_log::log_trace!("js_value->TransactionOutput: has_address:{has_address:?}");
            let value = object.get_u64("value")?;
            let script_public_key = ScriptPublicKey::try_from(object.get_value("scriptPublicKey")?)?;
            Ok(TransactionOutput::new(value, &script_public_key))
        } else {
            Err("TransactionInput must be an object".into())
        }
    }
}
