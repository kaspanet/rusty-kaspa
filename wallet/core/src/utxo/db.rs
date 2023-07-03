use super::{UtxoEntryId, UtxoEntryReference, UtxoSelectionContext};
use crate::imports::*;
use crate::result::Result;
// use js_sys::BigInt;
use crate::runtime::Balance;
use crate::wasm;
use kaspa_rpc_core::GetUtxosByAddressesResponse;
use serde_wasm_bindgen::from_value;
use sorted_insert::SortedInsertBinary;
use std::collections::HashMap;
use workflow_core::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum Disposition {
    Mature,
    Pending,
}

pub struct Consumed {
    entry: UtxoEntryReference,
    instant: Instant,
}

impl From<(UtxoEntryReference, &Instant)> for Consumed {
    fn from((entry, instant): (UtxoEntryReference, &Instant)) -> Self {
        Self { entry, instant: *instant }
    }
}

#[derive(Default)]
pub struct Inner {
    pub(crate) mature: Vec<UtxoEntryReference>,
    pub(crate) pending: HashMap<UtxoEntryId, UtxoEntryReference>,
    pub(crate) consumed: HashMap<UtxoEntryId, Consumed>,
    pub(crate) map: HashMap<UtxoEntryId, UtxoEntryReference>,
}

impl Inner {
    fn new() -> Self {
        Self { mature: vec![], pending: HashMap::default(), map: HashMap::default(), consumed: HashMap::default() }
    }

    fn new_with_args(entries: Vec<UtxoEntryReference>) -> Self {
        Self { mature: entries, pending: HashMap::default(), map: HashMap::default(), consumed: HashMap::default() }
    }
}

/// a collection of UTXO entries
#[derive(Clone, Default)]
#[wasm_bindgen]
pub struct UtxoDb {
    pub(crate) inner: Arc<Mutex<Inner>>,
}

#[wasm_bindgen]
impl UtxoDb {
    pub fn clear(&self) {
        let mut inner = self.inner();
        inner.map.clear();
        inner.mature.clear();
        inner.consumed.clear();
        inner.pending.clear();
    }
}

impl UtxoDb {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(Inner::new())) }
    }

    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub fn len(&self) -> usize {
        self.inner().mature.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn create_selection_context(&self) -> UtxoSelectionContext {
        UtxoSelectionContext::new(self.clone())
    }

    /// Insert `utxo_entry` into the `UtxoSet`.
    /// NOTE: The insert will be ignored if already present in the inner map.
    // pub fn insert(&self, utxo_entries: Vec<UtxoEntryReference>, current_daa_score : u64) {
    pub async fn insert(&self, utxo_entry: UtxoEntryReference, current_daa_score: u64) -> Result<Disposition> {
        let mut inner = self.inner();

        // for utxo_entry in utxo_entries.into_iter() {
        if let std::collections::hash_map::Entry::Vacant(e) = inner.map.entry(utxo_entry.id()) {
            e.insert(utxo_entry.clone());
            if utxo_entry.is_mature(current_daa_score) {
                inner.mature.sorted_insert_asc_binary(utxo_entry);
                Ok(Disposition::Mature)
            } else {
                inner.pending.insert(utxo_entry.id(), utxo_entry);
                Ok(Disposition::Pending)
            }
        } else {
            Err(Error::DuplicateUtxoEntry)
            // log_error!("ignoring duplicate utxo entry insert");
        }
        // }
    }

    pub async fn remove(&self, ids: Vec<UtxoEntryId>) {
        let mut inner = self.inner();

        // let mut removed_pending = vec![];
        let mut removed_mature_ids = vec![];
        for id in ids.into_iter() {
            if inner.map.remove(&id).is_some() && inner.pending.remove(&id).is_none() {
                removed_mature_ids.push(id);
            }
        }

        let remove_mature_ids = removed_mature_ids.into_iter().filter(|id| inner.consumed.remove(id).is_none()).collect::<Vec<_>>();
        inner.mature.retain(|entry| !remove_mature_ids.contains(&entry.id()));
    }

    pub fn promote(&self, utxo_entry: UtxoEntryReference) {
        log_info!("promote...");
        let id = utxo_entry.id();
        let mut inner = self.inner();
        if inner.pending.remove(&id).is_some() {
            log_info!("prnding remove ok...");
            inner.mature.sorted_insert_asc_binary(utxo_entry);
        } else {
            log_error!("Error: non-pending utxo promotion!");
        }
    }

    pub async fn extend(&self, utxo_entries: Vec<UtxoEntryReference>, current_daa_score: u64) -> Result<Vec<UtxoEntryReference>> {
        let mut pending = vec![];
        for entry in utxo_entries.into_iter() {
            let disposition = self.insert(entry.clone(), current_daa_score).await?;
            if matches!(disposition, Disposition::Pending) {
                pending.push(entry);
            };
        }
        Ok(pending)
    }

    pub async fn chunks(&self, chunk_size: usize) -> Result<Vec<Vec<UtxoEntryReference>>> {
        let entries = &self.inner().mature;
        let l = entries.chunks(chunk_size).map(|v| v.to_owned()).collect();
        Ok(l)
    }

    pub async fn recover(&self, duration: Option<Duration>) -> Result<()> {
        let checkpoint = Instant::now().checked_sub(duration.unwrap_or(Duration::from_secs(60))).unwrap();
        let mut inner = self.inner();
        let mut removed = vec![];
        inner.consumed.retain(|_, consumed| {
            if consumed.instant < checkpoint {
                removed.push(consumed.entry.clone());
                false
            } else {
                true
            }
        });

        removed.into_iter().for_each(|entry| {
            inner.mature.sorted_insert_asc_binary(entry);
        });

        Ok(())
    }

    /*

    // pub async fn select(&self, transaction_amount: u64, order: UtxoOrdering, mark_utxo: bool) -> Result<SelectionContext> {
    pub async fn select(&self, transaction_amount: u64, mark_utxo: bool) -> Result<SelectionContext> {
        // if self.ordered.load(Ordering::SeqCst) != order as u32 {
        //     self.order(order)?;
        // }

        // TODO: move to ticker callback
        self.update_inuse_utxos().await?;

        const FEE_PER_INPUT: u64 = 1124;

        let mut selected_entries = vec![];
        let mut in_use = vec![];
        let total_selected_amount = {
            let inner = self.inner();
            inner
                .entries
                .iter()
                .scan(0u64, |total, entry| {
                    let outpoint = entry.as_ref().outpoint.inner().clone();
                    if inner.in_use.contains_key(&outpoint) {
                        return Some(0);
                    }

                    if mark_utxo {
                        in_use.push(outpoint);
                    }
                    if *total >= transaction_amount + selected_entries.len() as u64 * FEE_PER_INPUT {
                        return None;
                    }

                    selected_entries.push(entry.clone());

                    let amount = entry.as_ref().utxo_entry.amount;
                    *total += amount;
                    Some(amount)
                })
                .sum()
        };

        if mark_utxo {
            let map = &mut self.inner().in_use;
            let now = Instant::now();
            for outpoint in in_use {
                map.insert(outpoint, now);
            }
        }

        Ok(SelectionContext { transaction_amount, total_selected_amount, selected_entries })
    }

    */

    pub async fn calculate_balance(&self) -> Balance {
        let mature = self.inner().mature.iter().map(|e| e.as_ref().entry.amount).sum();
        let pending = self.inner().pending.values().map(|e| e.as_ref().entry.amount).sum();
        Balance::new(mature, pending)
    }
}

#[wasm_bindgen]
impl UtxoDb {
    pub fn js_remove(&self, ids: Array) -> Result<Array> {
        let vec = ids.to_vec().iter().map(UtxoEntryId::try_from).collect::<Result<Vec<UtxoEntryId>>>()?;

        let mut inner = self.inner();

        let mut removed = vec![];
        for id in vec.iter() {
            if let Some(entry) = inner.map.remove(id) {
                removed.push(entry)
            }
        }

        for entry in removed.iter() {
            if inner.consumed.remove(&entry.id()).is_none() {
                inner.mature.retain(|entry| entry.id() != entry.id());
            }
        }

        Ok(removed.into_iter().map(JsValue::from).collect::<Array>())
    }

    // pub fn exists(&self, utxo_entry: &UtxoEntryReference) -> bool {
    //     let id = utxo_entry.id();
    //     self.inner.entries.lock().unwrap().iter().find(|entry| entry.id() == id).cloned().is_some()
    // }

    // pub fn find(&self, id: String) -> Option<UtxoEntryReference> {
    //     self.inner.entries.lock().unwrap().iter().find(|entry| entry.utxo.outpoint.id() == id).cloned()
    // }

    // #[wasm_bindgen(js_name=select)]
    // pub async fn select_utxos(&self, transaction_amount: u64, order: UtxoOrdering, mark_utxo: bool) -> Result<SelectionContext> {
    //     let data = self.select(transaction_amount, order, mark_utxo).await?;
    //     Ok(data)
    // }

    pub fn from(js_value: JsValue) -> Result<UtxoDb> {
        //log_info!("js_value: {:?}", js_value);
        let r: GetUtxosByAddressesResponse = from_value(js_value)?;
        //log_info!("r: {:?}", r);
        let mut entries = r.entries.into_iter().map(|entry| entry.into()).collect::<Vec<UtxoEntryReference>>();
        //log_info!("entries ...");
        entries.sort();

        let utxos = UtxoDb { inner: Arc::new(Mutex::new(Inner::new_with_args(entries))) };
        //log_info!("utxo_set ...");
        Ok(utxos)
    }

    #[wasm_bindgen(js_name=calculateBalance)]
    pub async fn js_calculate_balance(&self) -> wasm::Balance {
        self.calculate_balance().await.into()
    }

    #[wasm_bindgen(js_name=createSelectionContext)]
    pub fn js_create_selection_context(&self) -> UtxoSelectionContext {
        UtxoSelectionContext::new(self.clone())
    }
}
