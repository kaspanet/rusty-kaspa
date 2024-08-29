use crate::imports::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_TRANSACTION_OUTPUT: &'static str = r#"
/**
 * Interface defining the structure of a transaction output.
 * 
 * @category Consensus
 */
export interface ITransactionOutput {
    value: bigint;
    scriptPublicKey: IScriptPublicKey | HexString;

    /** Optional verbose data provided by RPC */
    verboseData?: ITransactionOutputVerboseData;
}

/**
 * TransactionOutput verbose data.
 * 
 * @category Node RPC
 */
export interface ITransactionOutputVerboseData {
    scriptPublicKeyType : string;
    scriptPublicKeyAddress : string;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "ITransactionOutput | TransactionOutput")]
    pub type TransactionOutputT;
    #[wasm_bindgen(typescript_type = "(ITransactionOutput | TransactionOutput)[]")]
    pub type TransactionOutputArrayAsArgT;
    #[wasm_bindgen(typescript_type = "TransactionOutput[]")]
    pub type TransactionOutputArrayAsResultT;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutputInner {
    pub value: u64,
    pub script_public_key: ScriptPublicKey,
}

/// Represents a Kaspad transaction output
/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionOutput {
    inner: Arc<Mutex<TransactionOutputInner>>,
}

impl TransactionOutput {
    pub fn new(value: u64, script_public_key: ScriptPublicKey) -> TransactionOutput {
        Self { inner: Arc::new(Mutex::new(TransactionOutputInner { value, script_public_key })) }
    }

    pub fn new_with_inner(inner: TransactionOutputInner) -> Self {
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn inner(&self) -> MutexGuard<'_, TransactionOutputInner> {
        self.inner.lock().unwrap()
    }

    pub fn script_public_key_length(&self) -> usize {
        self.inner().script_public_key.script().len()
    }
}

#[wasm_bindgen]
impl TransactionOutput {
    #[wasm_bindgen(constructor)]
    /// TransactionOutput constructor
    pub fn ctor(value: u64, script_public_key: &ScriptPublicKey) -> TransactionOutput {
        Self { inner: Arc::new(Mutex::new(TransactionOutputInner { value, script_public_key: script_public_key.clone() })) }
    }

    #[wasm_bindgen(getter, js_name = value)]
    pub fn value(&self) -> u64 {
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
}

impl AsRef<TransactionOutput> for TransactionOutput {
    fn as_ref(&self) -> &TransactionOutput {
        self
    }
}

impl From<cctx::TransactionOutput> for TransactionOutput {
    fn from(output: cctx::TransactionOutput) -> Self {
        TransactionOutput::new(output.value, output.script_public_key)
    }
}

impl From<&cctx::TransactionOutput> for TransactionOutput {
    fn from(output: &cctx::TransactionOutput) -> Self {
        TransactionOutput::new(output.value, output.script_public_key.clone())
    }
}

impl From<&TransactionOutput> for cctx::TransactionOutput {
    fn from(output: &TransactionOutput) -> Self {
        let inner = output.inner();
        cctx::TransactionOutput::new(inner.value, inner.script_public_key.clone())
    }
}

impl TryCastFromJs for TransactionOutput {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> std::result::Result<Cast<Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve_cast(value, || {
            if let Some(object) = Object::try_from(value.as_ref()) {
                let value = object.get_u64("value")?;
                let script_public_key = ScriptPublicKey::try_owned_from(object.get_value("scriptPublicKey")?)?;
                Ok(TransactionOutput::new(value, script_public_key).into())
            } else {
                Err("TransactionInput must be an object".into())
            }
        })
    }
}
