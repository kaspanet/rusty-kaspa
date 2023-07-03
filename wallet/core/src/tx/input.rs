use super::TransactionOutpoint;
use crate::imports::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInputInner {
    pub previous_outpoint: TransactionOutpoint,
    pub signature_script: Vec<u8>, // TODO: Consider using SmallVec
    pub sequence: u64,
    pub sig_op_count: u8,
}

/// Represents a Kaspa transaction input
#[derive(Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen(inspectable)]
pub struct TransactionInput {
    inner: Arc<Mutex<TransactionInputInner>>,
}

impl TransactionInput {
    pub fn new(previous_outpoint: TransactionOutpoint, signature_script: Vec<u8>, sequence: u64, sig_op_count: u8) -> Self {
        Self { inner: Arc::new(Mutex::new(TransactionInputInner { previous_outpoint, signature_script, sequence, sig_op_count })) }
    }

    pub fn new_with_inner(inner: TransactionInputInner) -> Self {
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn inner(&self) -> MutexGuard<'_, TransactionInputInner> {
        self.inner.lock().unwrap()
    }
}

#[wasm_bindgen]
impl TransactionInput {
    #[wasm_bindgen(constructor)]
    pub fn constructor(js_value: JsValue) -> Result<TransactionInput, JsError> {
        Ok(js_value.try_into()?)
    }

    #[wasm_bindgen(getter = previousOutpoint)]
    pub fn get_previous_outpoint(&self) -> TransactionOutpoint {
        self.inner().previous_outpoint.clone()
    }

    #[wasm_bindgen(setter = previousOutpoint)]
    pub fn set_previous_outpoint(&mut self, js_value: JsValue) {
        self.inner().previous_outpoint = js_value.try_into().expect("invalid signature script");
    }

    #[wasm_bindgen(getter = signatureScript)]
    pub fn get_signature_script_as_hex(&self) -> String {
        self.inner().signature_script.to_hex()
    }

    #[wasm_bindgen(setter = signatureScript)]
    pub fn set_signature_script_from_js_value(&mut self, js_value: JsValue) {
        self.inner().signature_script = js_value.try_as_vec_u8().expect("invalid signature script");
    }

    #[wasm_bindgen(getter = sequence)]
    pub fn get_sequence(&self) -> u64 {
        self.inner().sequence
    }

    #[wasm_bindgen(setter = sequence)]
    pub fn set_sequence(&mut self, sequence: u64) {
        self.inner().sequence = sequence;
    }

    #[wasm_bindgen(getter = sigOpCount)]
    pub fn get_sig_op_count(&self) -> u8 {
        self.inner().sig_op_count
    }

    #[wasm_bindgen(setter = sigOpCount)]
    pub fn set_sig_op_count(&mut self, sig_op_count: u8) {
        self.inner().sig_op_count = sig_op_count;
    }
}

impl TryFrom<JsValue> for TransactionInput {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&js_value) {
            let previous_outpoint: TransactionOutpoint = object.get("previousOutpoint")?.try_into()?;
            let signature_script = object.get_vec_u8("signatureScript")?;
            let sequence = object.get_u64("sequence")?;
            let sig_op_count = object.get_u8("sigOpCount")?;

            Ok(TransactionInput::new(previous_outpoint, signature_script, sequence, sig_op_count))
        } else {
            Err("TransactionInput must be an object".into())
        }
    }
}

impl From<cctx::TransactionInput> for TransactionInput {
    fn from(tx_input: cctx::TransactionInput) -> Self {
        TransactionInput::new_with_inner(TransactionInputInner {
            previous_outpoint: tx_input.previous_outpoint.into(),
            signature_script: tx_input.signature_script,
            sequence: tx_input.sequence,
            sig_op_count: tx_input.sig_op_count,
        })
    }
}

impl From<TransactionInput> for cctx::TransactionInput {
    fn from(tx_input: TransactionInput) -> Self {
        let inner = tx_input.inner();
        cctx::TransactionInput::new(
            inner.previous_outpoint.clone().into(),
            inner.signature_script.clone(),
            inner.sequence,
            inner.sig_op_count,
        )
    }
}
