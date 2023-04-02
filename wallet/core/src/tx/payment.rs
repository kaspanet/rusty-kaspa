use crate::imports::*;
use crate::tx::{TransactionOutput, TransactionOutputInner};
use crate::utxo::UtxoEntry;
use kaspa_txscript::pay_to_address_script;
use serde_wasm_bindgen::from_value;
use wasm_bindgen::convert::FromWasmAbi;

#[derive(Debug)]
#[wasm_bindgen(inspectable)]
#[allow(dead_code)] //TODO: remove me
pub struct PaymentOutput {
    #[wasm_bindgen(getter_with_clone)]
    pub address: Address,
    pub amount: u64,
    utxo_entry: Option<Arc<UtxoEntry>>,
}

#[wasm_bindgen]
impl PaymentOutput {
    #[wasm_bindgen(constructor)]
    pub fn new(address: Address, amount: u64, utxo_entry: Option<UtxoEntry>) -> Self {
        Self { address, amount, utxo_entry: utxo_entry.map(Arc::new) }
    }
}

impl From<PaymentOutput> for TransactionOutput {
    fn from(value: PaymentOutput) -> Self {
        Self::new_with_inner(TransactionOutputInner { script_public_key: pay_to_address_script(&value.address), value: value.amount })
    }
}

impl<'de> Deserialize<'de> for PaymentOutput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(OutputVisitor)
    }
}

struct OutputVisitor;

impl<'de> serde::de::Visitor<'de> for OutputVisitor {
    type Value = PaymentOutput;

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
pub struct PaymentOutputs {
    #[wasm_bindgen(skip)]
    pub outputs: Vec<PaymentOutput>,
}
#[wasm_bindgen]
impl PaymentOutputs {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(output_array: JsValue) -> crate::Result<PaymentOutputs> {
        let mut outputs = vec![];
        let iterator = js_sys::try_iter(&output_array)?.ok_or("need to pass iterable JS values!")?;
        for x in iterator {
            outputs.push(from_value(x?)?);
        }

        Ok(Self { outputs })
    }
}

impl TryFrom<JsValue> for PaymentOutputs {
    type Error = Error;
    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        let api = Object::from(value).get_u64("ptr")?;
        let outputs = unsafe { Self::from_abi(api as u32) };
        Ok(outputs)
    }
}

impl From<PaymentOutputs> for Vec<TransactionOutput> {
    fn from(value: PaymentOutputs) -> Self {
        value.outputs.into_iter().map(TransactionOutput::from).collect()
    }
}
