use crate::imports::*;
use crate::result::Result;
use crate::runtime::{Account, AccountId, Balance};
use crate::storage::TransactionType;
use crate::utxo::{Events, PendingUtxoEntryReference, UtxoEntryId, UtxoEntryReference, UtxoProcessorCore, UtxoSelectionContext};
// use kaspa_notify::scope::{Scope, UtxosChangedScope};
use crate::wasm;
use kaspa_rpc_core::GetUtxosByAddressesResponse;
use serde_wasm_bindgen::from_value;
use sorted_insert::SortedInsertBinary;
use std::collections::HashMap;
use workflow_core::time::{Duration, Instant};
use workflow_wasm::abi::ref_from_abi;

static PROCESSOR_ID_SEQUENCER: AtomicU64 = AtomicU64::new(0);
fn next_processor_id() -> u64 {
    PROCESSOR_ID_SEQUENCER.fetch_add(1, Ordering::SeqCst)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UtxoProcessorId(pub(crate) u64);

impl Default for UtxoProcessorId {
    fn default() -> Self {
        UtxoProcessorId(next_processor_id())
    }
}

impl From<AccountId> for UtxoProcessorId {
    fn from(id: AccountId) -> Self {
        UtxoProcessorId(id.0)
    }
}

impl From<&AccountId> for UtxoProcessorId {
    fn from(id: &AccountId) -> Self {
        UtxoProcessorId(id.0)
    }
}

impl UtxoProcessorId {
    pub fn new(id: u64) -> Self {
        UtxoProcessorId(id)
    }

    pub fn short(&self) -> String {
        let hex = self.to_hex();
        format!("{}..{}", &hex[0..4], &hex[hex.len() - 4..])
    }
}

impl ToHex for UtxoProcessorId {
    fn to_hex(&self) -> String {
        format!("{:x}", self.0)
    }
}

#[derive(Clone)]
pub enum Binding {
    Internal(UtxoProcessorId),
    Account(Arc<Account>),
    Id(UtxoProcessorId),
}

impl Default for Binding {
    fn default() -> Self {
        Binding::Internal(UtxoProcessorId::default())
    }
}

impl Binding {
    pub fn id(&self) -> UtxoProcessorId {
        match self {
            Binding::Internal(id) => *id,
            Binding::Account(account) => account.id().into(),
            Binding::Id(id) => *id,
        }
    }
}

// impl Binding {
//     pub fn id
// }

pub struct Consumed {
    entry: UtxoEntryReference,
    instant: Instant,
}

impl From<(UtxoEntryReference, &Instant)> for Consumed {
    fn from((entry, instant): (UtxoEntryReference, &Instant)) -> Self {
        Self { entry, instant: *instant }
    }
}

pub enum UtxoEntryVariant {
    Mature(UtxoEntryReference),
    Pending(UtxoEntryReference),
    Consumed(UtxoEntryReference),
}

// #[derive(Default)]
pub struct Inner {
    pub(crate) mature: Vec<UtxoEntryReference>,
    pub(crate) pending: HashMap<UtxoEntryId, UtxoEntryReference>,
    pub(crate) consumed: HashMap<UtxoEntryId, Consumed>,
    pub(crate) map: HashMap<UtxoEntryId, UtxoEntryReference>,
    balance: Option<Balance>,
    binding: Binding,
    addresses: Arc<DashSet<Arc<Address>>>,
}

impl Inner {
    fn new() -> Self {
        Self {
            mature: vec![],
            pending: HashMap::default(),
            map: HashMap::default(),
            consumed: HashMap::default(),
            balance: None,
            binding: Binding::default(),
            addresses: Arc::new(DashSet::default()),
        }
    }

    fn new_with_mature(entries: Vec<UtxoEntryReference>) -> Self {
        Self {
            mature: entries,
            pending: HashMap::default(),
            map: HashMap::default(),
            consumed: HashMap::default(),
            balance: None,
            binding: Binding::default(),
            addresses: Arc::new(DashSet::default()),
        }
    }
}

/// a collection of UTXO entries
#[derive(Clone)]
#[wasm_bindgen]
pub struct UtxoProcessor {
    pub(crate) inner: Arc<Mutex<Inner>>,
    pub(crate) core: UtxoProcessorCore,
    // pub(crate) id : Id,
}

#[wasm_bindgen]
impl UtxoProcessor {
    pub async fn clear(&self) -> Result<()> {
        let local = self.addresses();
        let addresses = local.iter().map(|v| v.clone()).collect::<Vec<_>>();
        if !addresses.is_empty() {
            self.core.unregister_addresses(addresses).await?;
            local.clear();
        }

        let mut inner = self.inner();
        inner.map.clear();
        inner.mature.clear();
        inner.consumed.clear();
        inner.pending.clear();
        inner.addresses.clear();
        inner.balance = None;

        Ok(())
    }
}

impl UtxoProcessor {
    pub fn new(core: &UtxoProcessorCore) -> Self {
        Self { inner: Arc::new(Mutex::new(Inner::new())), core: core.clone() }
    }

    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub fn bind_to_account(&self, account: &Arc<Account>) {
        self.inner().binding = Binding::Account(account.clone());
    }

    pub fn bind_to_id(&self, id: UtxoProcessorId) {
        self.inner().binding = Binding::Id(id);
    }

    // pub fn account(&self) -> Option<Arc<Account>> {
    //     self.inner().account.clone()
    // }

    pub fn binding(&self) -> Binding {
        self.inner().binding.clone()
    }

    pub fn id(&self) -> UtxoProcessorId {
        self.binding().id()
    }

    pub fn mature_utxo_size(&self) -> usize {
        self.inner().mature.len()
    }

    pub fn pending_utxo_size(&self) -> usize {
        self.inner().pending.len()
    }

    pub fn balance(&self) -> Option<Balance> {
        self.inner().balance.clone()
    }

    pub fn addresses(&self) -> Arc<DashSet<Arc<Address>>> {
        self.inner().addresses.clone()
    }

    pub async fn update_balance(&self) -> Result<Balance> {
        let (balance, mature_utxo_size, pending_utxo_size) = {
            let previous_balance = self.balance();
            let mut balance = self.calculate_balance().await;
            balance.delta(&previous_balance);

            let mut inner = self.inner();
            inner.balance.replace(balance.clone());
            let mature_utxo_size = inner.mature.len();
            let pending_utxo_size = inner.pending.len();

            (balance, mature_utxo_size, pending_utxo_size)
        };

        self.core
            .notify(Events::Balance { balance: Some(balance.clone()), id: self.id(), mature_utxo_size, pending_utxo_size })
            .await?;

        Ok(balance)
    }

    pub fn create_selection_context(&self) -> UtxoSelectionContext {
        UtxoSelectionContext::new(self)
    }

    /// Insert `utxo_entry` into the `UtxoSet`.
    /// NOTE: The insert will be ignored if already present in the inner map.
    // pub fn insert(&self, utxo_entries: Vec<UtxoEntryReference>, current_daa_score : u64) {
    pub async fn insert(
        &self,
        utxo_entry: UtxoEntryReference,
        current_daa_score: u64,
        // ctx: Option<&UtxoProcessorContext>,
    ) -> Result<()> {
        let mut inner = self.inner();

        // for utxo_entry in utxo_entries.into_iter() {
        if let std::collections::hash_map::Entry::Vacant(e) = inner.map.entry(utxo_entry.id()) {
            e.insert(utxo_entry.clone());
            if utxo_entry.is_mature(current_daa_score) {
                inner.mature.sorted_insert_asc_binary(utxo_entry);
                Ok(())
            } else {
                inner.pending.insert(utxo_entry.id(), utxo_entry.clone());
                self.core.pending().insert(utxo_entry.id(), PendingUtxoEntryReference::new(utxo_entry, self.clone()));
                Ok(())
            }
        } else {
            Err(Error::DuplicateUtxoEntry)
            // log_error!("ignoring duplicate utxo entry insert");
        }

        // }
    }

    pub async fn remove(&self, ids: Vec<UtxoEntryId>) -> Result<Vec<UtxoEntryVariant>> {
        let mut inner = self.inner();

        let mut removed = vec![];

        let mut remove_mature_ids = vec![];
        for id in ids.into_iter() {
            // remove from local map
            if inner.map.remove(&id).is_some() {
                if let Some(pending) = inner.pending.remove(&id) {
                    removed.push(UtxoEntryVariant::Pending(pending));
                    if self.core.pending().remove(&id).is_none() {
                        log_error!("Error: unable to remove utxo entry from global pending (with context)");
                    }
                } else {
                    remove_mature_ids.push(id);
                }

                // remove from local pending
                // if inner.pending.remove(&id).is_none() {
                //     // if item was not in local pending, it is in the mature list
                //     removed_mature_ids.push(id);
                // } else {
                //     // if item was in local pending, it is also in global pending
                //     if self.core.pending().remove(&id).is_none() {
                //         log_error!("Error: unable to remove utxo entry from global pending (with context)");
                //     }
                // }
            } else {
                log_error!("Error: unable to remove utxo entry from local map (with context)");
            }
        }

        let remove_mature_ids = remove_mature_ids
            .into_iter()
            .filter(|id| {
                // inner.consumed.remove(id).is_none()
                if let Some(consumed) = inner.consumed.remove(id) {
                    removed.push(UtxoEntryVariant::Consumed(consumed.entry));
                    false
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();
        inner.mature.retain(|entry| {
            if remove_mature_ids.contains(&entry.id()) {
                removed.push(UtxoEntryVariant::Mature(entry.clone()));
                false
            } else {
                true
            }
        });

        Ok(removed)
    }

    pub fn promote(&self, utxo_entry: UtxoEntryReference) {
        let id = utxo_entry.id();
        let mut inner = self.inner();
        if inner.pending.remove(&id).is_some() {
            inner.mature.sorted_insert_asc_binary(utxo_entry);
        } else {
            log_error!("Error: non-pending utxo promotion!");
        }
    }

    pub async fn extend(
        &self,
        utxo_entries: Vec<UtxoEntryReference>,
        current_daa_score: u64,
        // ctx: Option<&UtxoProcessorContext>,
    ) -> Result<()> {
        //Result<Vec<UtxoEntryReference>> {
        // let mut pending = vec![];
        for entry in utxo_entries.into_iter() {
            // let disposition =
            self.insert(entry.clone(), current_daa_score).await?;
            // if matches!(disposition, Disposition::Pending) {
            //     pending.push(entry);
            // };
        }
        Ok(())
        // Ok(pending)
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

    pub(crate) async fn handle_utxo_added(&self, utxos: Vec<UtxoEntryReference>) -> Result<()> {
        // add UTXOs to account set
        let current_daa_score = self.core.current_daa_score();

        for utxo in utxos.iter() {
            // match
            if let Err(err) = self.insert(utxo.clone(), current_daa_score).await {
                log_error!("{}", err);
            }
        }

        for utxo in utxos.into_iter() {
            // post update notifications
            let record = (TransactionType::Credit, self, utxo).into();
            self.core.notify(Events::Pending { record }).await?;
        }
        // post balance update
        self.update_balance().await?;
        Ok(())
    }

    pub(crate) async fn handle_utxo_removed(&self, utxos: Vec<UtxoEntryReference>) -> Result<()> {
        // let current_daa_score = self.core.current_daa_score();

        // remove UTXOs from account set
        let utxo_ids: Vec<UtxoEntryId> = utxos.iter().map(|utxo| utxo.id()).collect();
        let removed = self.remove(utxo_ids).await?;

        for removed in removed.into_iter() {
            match removed {
                UtxoEntryVariant::Mature(utxo) => {
                    let record = (TransactionType::Debit, self, utxo).into();
                    self.core.notify(Events::External { record }).await?;
                }
                UtxoEntryVariant::Consumed(_utxo) => {
                    // let record = (TransactionType::Debit, self, utxo).into();
                    // self.core.notify(Events::Debit { record }).await?;
                }
                UtxoEntryVariant::Pending(utxo) => {
                    let record = (TransactionType::Reorg, self, utxo).into();
                    self.core.notify(Events::Reorg { record }).await?;
                }
            }
        }

        // post update notifications
        // for utxo in utxos.into_iter() {
        //     let record = (TransactionType::Debit, self, utxo).into();
        //     self.core.notify(Events::DebitPending { record }).await?;
        // }

        // post balance update
        self.update_balance().await?;
        Ok(())
    }

    // async fn subscribe_utxos_changed(self: &Arc<Self>, addresses: &[Address]) -> Result<()> {
    pub async fn register_addresses(self: &Arc<Self>, addresses: Vec<Address>) -> Result<()> {
        // log_info!("registering addresses (1) {:#?}", addresses);

        let local = self.addresses();

        let addresses = addresses
            .clone()
            .into_iter()
            .filter_map(|address| {
                let address = Arc::new(address);
                if local.insert(address.clone()) {
                    Some(address)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // log_info!("registering addresses (2) {:#?}", addresses);

        // self.inner().addresses.extend(addresses.clone());
        self.core.register_addresses(addresses, self).await?;
        // self.wallet.utxo_processor_core().address_to_account_map().extend(addresses.iter().map(|a| (a.clone(), self.clone())));
        // let listener_id = self.core.listener_id();
        // // for address in addresses.iter() {
        // //     log_info!("{}: subscribing to {}", self.id, address);
        // // }
        // let utxos_changed_scope = UtxosChangedScope { addresses };
        // self.wallet.rpc().start_notify(listener_id, Scope::UtxosChanged(utxos_changed_scope)).await?;

        Ok(())
    }

    pub async fn unregister_addresses(self: &Arc<Self>, addresses: Vec<Address>) -> Result<()> {
        if !addresses.is_empty() {
            // log_info!("unregistering addresses (1) {:#?}", addresses);
            let local = self.addresses();

            let addresses = addresses.clone().into_iter().map(Arc::new).collect::<Vec<_>>();
            // log_info!("unregistering addresses (2) {:#?}", addresses);

            self.core.unregister_addresses(addresses.clone()).await?;
            addresses.iter().for_each(|address| {
                local.remove(address);
            });
        } else {
            log_warning!("utxo processor: unregistering empty address set")
        }
        // let addresses = addresses.clone().into_iter().filter_map(|address|{
        //     let address = Arc::new(address);
        //     local.remove(&address)
        // }).collect::<Vec<_>>();

        // self.wallet.utxo_processor_core().unregister_addresses(addresses.clone().into_iter().map(Arc::new).collect());
        // // self.wallet.address_to_account_map().lock().unwrap().extend(addresses.iter().map(|a| (a.clone(), self.clone())));

        // let listener_id = self
        //     .listener_id()
        //     .expect("subscribe_utxos_changed() requires `listener_id` (must call `register_notification_listener()` before use)");
        // let utxos_changed_scope = UtxosChangedScope { addresses: addresses.to_vec() };
        // self.wallet.rpc().stop_notify(listener_id, Scope::UtxosChanged(utxos_changed_scope)).await?;

        Ok(())
    }
}

#[wasm_bindgen]
impl UtxoProcessor {
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

    // #[wasm_bindgen(constructor)]
    // pub fn constructor(core: &JsValue, id_or_account : &JsValue, utxo_by_address_response: JsValue) -> Result<UtxoProcessor> {
    pub fn from(core: &JsValue, utxo_by_address_response: JsValue) -> Result<UtxoProcessor> {
        // pub fn from(utxo_by_address_response: JsValue) -> Result<UtxoProcessor> {
        //log_info!("js_value: {:?}", js_value);
        let core = ref_from_abi!(UtxoProcessorCore, core)?;
        // let id_or_account =

        // Id::new(id_or_account.try_as_u64()?);
        let r: GetUtxosByAddressesResponse = from_value(utxo_by_address_response)?;
        //log_info!("r: {:?}", r);
        let mut entries = r.entries.into_iter().map(|entry| entry.into()).collect::<Vec<UtxoEntryReference>>();
        //log_info!("entries ...");
        entries.sort();

        let utxos = UtxoProcessor {
            inner: Arc::new(Mutex::new(Inner::new_with_mature(entries))),
            // id,
            core, // : None,
        };
        //log_info!("utxo_set ...");
        Ok(utxos)
    }

    #[wasm_bindgen(js_name=calculateBalance)]
    pub async fn js_calculate_balance(&self) -> wasm::Balance {
        self.calculate_balance().await.into()
    }

    #[wasm_bindgen(js_name=createSelectionContext)]
    pub fn js_create_selection_context(&self) -> UtxoSelectionContext {
        UtxoSelectionContext::new(self)
    }
}
