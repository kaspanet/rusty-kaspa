use crate::tx::ScriptPublicKey;
use crate::utxo::UtxoEntry;
use kaspa_addresses::Address;
use serde::Deserializer;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::from_value;
use std::sync::{Arc, Mutex, MutexGuard};
use wasm_bindgen::convert::FromWasmAbi;
use wasm_bindgen::prelude::*;

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

    fn inner(&self) -> MutexGuard<'_, TransactionOutputInner> {
        self.inner.lock().unwrap()
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
