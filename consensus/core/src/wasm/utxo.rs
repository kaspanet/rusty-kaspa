use crate::tx::{self, ScriptPublicKey, UtxoEntry};
use crate::wasm::error::Error;
use itertools::Itertools;
use js_sys::Array;
use js_sys::Object;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use workflow_wasm::abi::ref_from_abi;
use workflow_wasm::object::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen(js_name = "TxUtxoEntryList")]
pub struct UtxoEntryList(Arc<Vec<tx::UtxoEntry>>);

#[wasm_bindgen]
impl UtxoEntryList {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(js_value: JsValue) -> Result<UtxoEntryList, JsError> {
        Ok(js_value.try_into()?)
    }
    #[wasm_bindgen(getter = items)]
    pub fn get_items_as_js_array(&self) -> JsValue {
        let items = self.0.as_ref().clone().into_iter().map(<UtxoEntry as Into<JsValue>>::into);
        Array::from_iter(items).into()
    }

    #[wasm_bindgen(setter = items)]
    pub fn set_items_from_js_array(&mut self, js_value: &JsValue) {
        let items = Array::from(js_value)
            .iter()
            .map(|js_value| ref_from_abi!(UtxoEntry, &js_value).unwrap_or_else(|err| panic!("invalid UTXOEntry: {err}")))
            .collect::<Vec<_>>();
        self.0 = Arc::new(items);
    }
}

impl From<UtxoEntryList> for Vec<Option<UtxoEntry>> {
    fn from(value: UtxoEntryList) -> Self {
        value.0.as_ref().iter().map(|entry| Some(entry.clone())).collect_vec()
    }
}

impl TryFrom<Vec<Option<UtxoEntry>>> for UtxoEntryList {
    type Error = Error;
    fn try_from(value: Vec<Option<UtxoEntry>>) -> Result<Self, Self::Error> {
        let mut list = vec![];
        for entry in value.into_iter() {
            list.push(entry.ok_or(Error::Custom("Unable to cast `Vec<Option<UtxoEntry>>` into `UtxoEntryList`.".to_string()))?);
        }

        Ok(Self(Arc::new(list)))
    }
}

impl TryFrom<JsValue> for UtxoEntryList {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        if !js_value.is_array() {
            return Err("UtxoEntryList must be an array".into());
        }

        let mut list = vec![];
        for entry in Array::from(&js_value).iter() {
            list.push(match ref_from_abi!(UtxoEntry, &entry) {
                Ok(value) => value,
                Err(err) => {
                    if !entry.is_object() {
                        panic!("invalid UTXOEntry: {err}")
                    }
                    //log_trace!("entry: {:?}", entry);
                    let object = Object::from(entry);
                    let amount = object.get_u64("amount")?;
                    let script_public_key: ScriptPublicKey =
                        object.get("scriptPublicKey").map_err(|_| Error::Custom("missing `script` property".into()))?.try_into()?;
                    let block_daa_score = object.get_u64("blockDaaScore")?;
                    let is_coinbase = object.get_bool("isCoinbase")?;
                    tx::UtxoEntry { amount, script_public_key, block_daa_score, is_coinbase }
                }
            })
        }
        Ok(UtxoEntryList(Arc::new(list)))
    }
}
