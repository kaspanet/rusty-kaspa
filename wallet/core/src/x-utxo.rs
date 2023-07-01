use crate::imports::*;
use crate::result::Result;
use crate::tx::{TransactionOutpoint, TransactionOutpointInner};
use itertools::Itertools;
use kaspa_rpc_core::{GetUtxosByAddressesResponse, RpcUtxosByAddressesEntry};
use serde_wasm_bindgen::from_value;
use sorted_insert::SortedInsertBinary;
use std::cmp::Ordering;
use std::collections::HashMap;
use workflow_core::time::{Duration, Instant};
use workflow_wasm::abi::{ref_from_abi, TryFromJsValue};


// #[wasm_bindgen]
// /// Result containing data produced by the `UtxoSet::select()` function
// pub struct SelectionContext {
//     #[wasm_bindgen(js_name = "amount")]
//     pub transaction_amount: u64,
//     #[wasm_bindgen(js_name = "totalAmount")]
//     pub total_selected_amount: u64,
//     #[wasm_bindgen(skip)]
//     pub selected_entries: Vec<UtxoEntryReference>,
// }

// #[wasm_bindgen]
// impl SelectionContext {
//     #[wasm_bindgen(getter=utxos)]
//     pub fn selected_entries(&self) -> js_sys::Array {
//         js_sys::Array::from_iter(self.selected_entries.clone().into_iter().map(JsValue::from))
//     }
// }

// /// UtxoOrdering enum denotes UTXO sort order (`Unordered`, `AscendingAmount`, `AscendingDaaScore`)
// #[derive(Default, Clone, Copy)]
// #[repr(u32)]
// #[wasm_bindgen]
// pub enum UtxoOrdering {
//     #[default]
//     Unordered,
//     AscendingAmount,
//     AscendingDaaScore,
// }

use cctx::ScriptPublicKey;
use js_sys::Array;
use js_sys::Object;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// #[derive(Clone, Debug, Serialize, Deserialize)]
// #[wasm_bindgen(js_name = "TxUtxoEntryList")]
// pub struct UtxoEntryList(Arc<Vec<cctx::UtxoEntry>>);

// #[wasm_bindgen]
// impl UtxoEntryList {
//     #[wasm_bindgen(constructor)]
//     pub fn js_ctor(js_value: JsValue) -> std::result::Result<UtxoEntryList, JsError> {
//         Ok(js_value.try_into()?)
//     }
//     #[wasm_bindgen(getter = items)]
//     pub fn get_items_as_js_array(&self) -> JsValue {
//         let items = self.0.as_ref().clone().into_iter().map(<cctx::UtxoEntry as Into<JsValue>>::into);
//         Array::from_iter(items).into()
//     }

//     #[wasm_bindgen(setter = items)]
//     pub fn set_items_from_js_array(&mut self, js_value: &JsValue) {
//         let items = Array::from(js_value)
//             .iter()
//             .map(|js_value| ref_from_abi!(cctx::UtxoEntry, &js_value).unwrap_or_else(|err| panic!("invalid UTXOEntry: {err}")))
//             .collect::<Vec<_>>();
//         self.0 = Arc::new(items);
//     }
// }

// impl From<UtxoEntryList> for Vec<Option<cctx::UtxoEntry>> {
//     fn from(value: UtxoEntryList) -> Self {
//         value.0.as_ref().iter().map(|entry| Some(entry.clone())).collect_vec()
//     }
// }

// impl TryFrom<Vec<Option<cctx::UtxoEntry>>> for UtxoEntryList {
//     type Error = Error;
//     fn try_from(value: Vec<Option<cctx::UtxoEntry>>) -> Result<Self> {
//         let mut list = vec![];
//         for entry in value.into_iter() {
//             list.push(entry.ok_or(Error::Custom("Unable to cast `Vec<Option<UtxoEntry>>` into `UtxoEntryList`.".to_string()))?);
//         }

//         Ok(Self(Arc::new(list)))
//     }
// }

// impl TryFrom<JsValue> for UtxoEntryList {
//     type Error = Error;
//     fn try_from(js_value: JsValue) -> Result<Self> {
//         if !js_value.is_array() {
//             return Err("UtxoEntryList must be an array".into());
//         }

//         let mut list = vec![];
//         for entry in Array::from(&js_value).iter() {
//             list.push(match ref_from_abi!(cctx::UtxoEntry, &entry) {
//                 Ok(value) => value,
//                 Err(err) => {
//                     if !entry.is_object() {
//                         panic!("invalid UTXOEntry: {err}")
//                     }
//                     //log_trace!("entry: {:?}", entry);
//                     let object = Object::from(entry);
//                     let amount = object.get_u64("amount")?;
//                     let script_public_key = ScriptPublicKey::try_from_jsvalue(
//                         object.get("scriptPublicKey").map_err(|_| Error::Custom("missing `scriptPublicKey` property".into()))?,
//                     )?;
//                     let block_daa_score = object.get_u64("blockDaaScore")?;
//                     let is_coinbase = object.get_bool("isCoinbase")?;
//                     cctx::UtxoEntry { amount, script_public_key, block_daa_score, is_coinbase }
//                 }
//             })
//         }
//         Ok(UtxoEntryList(Arc::new(list)))
//     }
// }
