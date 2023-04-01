use crate::imports::*;
use crate::utxo::UtxoEntry;
use kaspa_txscript::pay_to_address_script;
use serde_wasm_bindgen::from_value;
use wasm_bindgen::convert::FromWasmAbi;

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

    //     pub fn new(value: u64, script_public_key: ScriptPublicKey) -> Self {
    //         Self { inner : Arc::new(Mutex::new(TransactionOutputInner { value, script_public_key })) }
    //     }
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
}

impl TryFrom<cctx::TransactionOutput> for TransactionOutput {
    type Error = Error;
    fn try_from(output: cctx::TransactionOutput) -> Result<Self, Self::Error> {
        Ok(TransactionOutput::new(output.value, &output.script_public_key))
    }
}

impl TryFrom<TransactionOutput> for cctx::TransactionOutput {
    type Error = Error;
    fn try_from(output: TransactionOutput) -> Result<Self, Self::Error> {
        let inner = output.inner();
        Ok(cctx::TransactionOutput::new(inner.value, inner.script_public_key.clone()))
    }
}

impl TryFrom<JsValue> for TransactionOutput {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        if js_value.is_object() {
            workflow_log::log_trace!("js_value->TransactionOutput: {js_value:?}");
            let object = Object::from(js_value);
            let has_address = Object::has_own(&object, &JsValue::from("address"));
            workflow_log::log_trace!("js_value->TransactionOutput: has_address:{has_address:?}");
            let value = object.get_u64("value")?;
            let script_public_key: ScriptPublicKey =
                object.get("scriptPublicKey").map_err(|_| Error::Custom("missing `script` property".into()))?.try_into()?;
            Ok(TransactionOutput::new(value, &script_public_key))
        } else {
            Err("TransactionInput must be an object".into())
        }
    }
}

// ~~~

#[derive(Debug)]
#[wasm_bindgen(inspectable)]
#[allow(dead_code)] //TODO: remove me
pub struct Output {
    #[wasm_bindgen(getter_with_clone)]
    pub address: Address,
    pub amount: u64,
    utxo_entry: Option<Arc<UtxoEntry>>,
}

#[wasm_bindgen]
impl Output {
    #[wasm_bindgen(constructor)]
    pub fn new(address: Address, amount: u64, utxo_entry: Option<UtxoEntry>) -> Self {
        Self { address, amount, utxo_entry: utxo_entry.map(Arc::new) }
    }
}

impl From<Output> for TransactionOutput {
    fn from(value: Output) -> Self {
        Self::new_with_inner(TransactionOutputInner { script_public_key: pay_to_address_script(&value.address), value: value.amount })
    }
}

impl<'de> Deserialize<'de> for Output {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(OutputVisitor)
    }
}

struct OutputVisitor;

impl<'de> serde::de::Visitor<'de> for OutputVisitor {
    type Value = Output;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "valid Output object.")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let key = map.next_key::<String>()?;
        let value = map.next_value::<u32>()?;

        if let Some(key) = &key {
            if key.eq("ptr") {
                return Ok(unsafe { Self::Value::from_abi(value) });
            }
        }
        Err(serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self))
        //Err(serde::de::Error::invalid_value(serde::de::Unexpected::Str(&format!("Invalid address: {{{key:?}:{value:?}}}")), &self))
    }
}

#[derive(Debug)]
#[wasm_bindgen]
pub struct Outputs {
    #[wasm_bindgen(skip)]
    pub outputs: Vec<Output>,
}
#[wasm_bindgen]
impl Outputs {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(output_array: JsValue) -> crate::Result<Outputs> {
        let mut outputs = vec![];
        let iterator = js_sys::try_iter(&output_array)?.ok_or("need to pass iterable JS values!")?;
        for x in iterator {
            outputs.push(from_value(x?)?);
        }

        Ok(Self { outputs })
    }
}

impl TryFrom<JsValue> for Outputs {
    type Error = Error;
    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        let api = Object::from(value).get_u64("ptr")?;
        let outputs = unsafe { Self::from_abi(api as u32) };
        Ok(outputs)
    }
}

impl From<Outputs> for Vec<TransactionOutput> {
    fn from(value: Outputs) -> Self {
        value.outputs.into_iter().map(TransactionOutput::from).collect()
    }
}
