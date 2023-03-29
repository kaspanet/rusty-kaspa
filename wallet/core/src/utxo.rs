use crate::error::Error;
use crate::result::Result;
use itertools::Itertools;
use js_sys::{Array, Object};
use kaspa_consensus_core::tx::{self, ScriptPublicKey, TransactionOutpoint};
use kaspa_rpc_core::RpcUtxosByAddressesEntry;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};
use wasm_bindgen::prelude::*;
use workflow_wasm::abi::ref_from_abi;
use workflow_wasm::object::*;

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use kaspa_addresses::Address;
use serde::{Deserialize, Serialize};

// #[derive(Clone, TryFromJsValue)]
// #[wasm_bindgen]
// pub struct UtxoEntryReference {
//     #[wasm_bindgen(skip)]
//     pub utxo: Arc<UtxoEntry>,
// }

// impl AsRef<UtxoEntry> for UtxoEntryReference {
//     fn as_ref(&self) -> &UtxoEntry {
//         &self.utxo
//     }
// }

// pub struct SelectionContext {
//     pub transaction_amount: u64,
//     pub total_selected_amount: u64,
//     pub selected_entries: Vec<UtxoEntryReference>,
// }

// impl AsMut<UtxoEntry> for AccountUtxoEntry {
//     fn as_mut(&mut self) -> &mut UtxoEntry {
//         &mut self.0
//     }
// }

///
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub struct UtxoEntry {
    #[wasm_bindgen(getter_with_clone)]
    pub address: Address,
    pub outpoint: tx::TransactionOutpoint,
    #[wasm_bindgen(getter_with_clone)]
    pub utxo_entry: tx::UtxoEntry,
}

impl UtxoEntry {
    #[inline(always)]
    pub fn amount(&self) -> u64 {
        self.utxo_entry.amount
    }
    #[inline(always)]
    pub fn block_daa_score(&self) -> u64 {
        self.utxo_entry.block_daa_score
    }
}

impl From<RpcUtxosByAddressesEntry> for UtxoEntry {
    fn from(entry: RpcUtxosByAddressesEntry) -> UtxoEntry {
        UtxoEntry { address: entry.address, outpoint: entry.outpoint, utxo_entry: entry.utxo_entry }
    }
}

// #[derive(Clone, TryFromJsValue)]
#[derive(Clone)]
// #[wasm_bindgen]
pub struct UtxoEntryReference {
    // #[wasm_bindgen(skip)]
    pub utxo: Arc<UtxoEntry>,
}

impl AsRef<UtxoEntry> for UtxoEntryReference {
    fn as_ref(&self) -> &UtxoEntry {
        &self.utxo
    }
}

impl From<UtxoEntryReference> for UtxoEntry {
    fn from(value: UtxoEntryReference) -> Self {
        (*value.utxo).clone()
    }
}

pub struct SelectionContext {
    pub transaction_amount: u64,
    pub total_selected_amount: u64,
    pub selected_entries: Vec<UtxoEntryReference>,
}

#[derive(Clone, Copy)]
#[repr(u32)]
#[wasm_bindgen]
pub enum UtxoOrdering {
    Unordered,
    AscendingAmount,
    AscendingDaaScore,
}

#[derive(Default)]
pub struct Inner {
    entries: Mutex<Vec<UtxoEntryReference>>,
    ordered: AtomicU32,
}

/// a collection of UTXO entries
#[derive(Clone, Default)]
#[wasm_bindgen]
pub struct UtxoSet {
    inner: Arc<Inner>,
}

impl UtxoSet {
    pub fn insert(&mut self, utxo_entry: UtxoEntryReference) {
        self.inner.entries.lock().unwrap().push(utxo_entry);
        self.inner.ordered.store(UtxoOrdering::Unordered as u32, Ordering::SeqCst);
    }

    pub fn order(&self, order: UtxoOrdering) -> Result<()> {
        match order {
            UtxoOrdering::AscendingAmount => {
                //self.inner.entries.lock().unwrap().sort_by(|a, b| a.as_ref().amount().cmp(&b.as_ref().amount()));
                self.inner.entries.lock().unwrap().sort_by_key(|a| a.as_ref().amount());
            }
            UtxoOrdering::AscendingDaaScore => {
                //self.inner.entries.lock().unwrap().sort_by(|a, b| a.as_ref().block_daa_score().cmp(&b.as_ref().block_daa_score()));
                self.inner.entries.lock().unwrap().sort_by_key(|a| a.as_ref().block_daa_score());
            }
            UtxoOrdering::Unordered => {
                // Ok(self.entries)
            }
        }

        Ok(())
    }

    pub async fn chunks(&self, chunk_size: usize) -> Result<Vec<Vec<UtxoEntryReference>>> {
        let entries = self.inner.entries.lock().unwrap();
        let l = entries.chunks(chunk_size).map(|v| v.to_owned()).collect();
        Ok(l)
    }

    pub async fn select(&self, transaction_amount: u64, order: UtxoOrdering) -> Result<SelectionContext> {
        if self.inner.ordered.load(Ordering::SeqCst) != order as u32 {
            self.order(order)?;
        }

        let mut selected_entries = vec![];

        let total_selected_amount = self
            .inner
            .entries
            .lock()
            .unwrap()
            .iter()
            .scan(0u64, |total, entry| {
                if *total >= transaction_amount {
                    return None;
                }

                selected_entries.push(entry.clone());

                let amount = entry.as_ref().utxo_entry.amount;
                *total += amount;
                Some(amount)
            })
            .sum();

        Ok(SelectionContext { transaction_amount, total_selected_amount, selected_entries })

        // TODO - untested!
    }

    pub async fn calculate_balance(&self) -> Result<u64> {
        Ok(self.inner.entries.lock().unwrap().iter().map(|e| e.as_ref().utxo_entry.amount).sum())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct UtxoEntries(Arc<Vec<UtxoEntry>>);

#[wasm_bindgen]
impl UtxoEntries {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(js_value: JsValue) -> Result<UtxoEntries> {
        js_value.try_into()
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

impl From<UtxoEntries> for Vec<Option<UtxoEntry>> {
    fn from(value: UtxoEntries) -> Self {
        value.0.as_ref().iter().map(|entry| Some(entry.clone())).collect_vec()
    }
}

impl From<UtxoEntries> for Vec<Option<tx::UtxoEntry>> {
    fn from(value: UtxoEntries) -> Self {
        value.0.as_ref().iter().map(|entry| Some(entry.utxo_entry.clone())).collect_vec()
    }
}

impl TryFrom<Vec<Option<UtxoEntry>>> for UtxoEntries {
    type Error = Error;
    fn try_from(value: Vec<Option<UtxoEntry>>) -> std::result::Result<Self, Self::Error> {
        let mut list = vec![];
        for entry in value.into_iter() {
            list.push(entry.ok_or(Error::Custom("Unable to cast `Vec<Option<UtxoEntry>>` into `UtxoEntryList`.".to_string()))?);
        }

        Ok(Self(Arc::new(list)))
    }
}

impl TryFrom<Vec<UtxoEntryReference>> for UtxoEntries {
    type Error = Error;
    fn try_from(value: Vec<UtxoEntryReference>) -> std::result::Result<Self, Self::Error> {
        let mut list = vec![];
        for entry in value.into_iter() {
            list.push(
                entry
                    .try_into()
                    .map_err(|_| Error::Custom("Unable to cast `Vec<UtxoEntryReference>` into `UtxoEntryList`.".to_string()))?,
            );
        }

        Ok(Self(Arc::new(list)))
    }
}

impl TryFrom<JsValue> for UtxoEntries {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
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
                    let address: Address = object.get_string("address")?.try_into()?;
                    let outpoint: TransactionOutpoint = object.get("outpoint")?.try_into()?;
                    UtxoEntry {
                        address,
                        outpoint,
                        utxo_entry: tx::UtxoEntry { amount, script_public_key, block_daa_score, is_coinbase },
                    }
                }
            })
        }
        Ok(Self(Arc::new(list)))
    }
}
