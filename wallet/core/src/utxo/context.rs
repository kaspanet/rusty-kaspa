use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{AccountId, Balance};
use crate::storage::{TransactionRecord, TransactionType};
use crate::tx::PendingTransaction;
use crate::utxo::{
    PendingUtxoEntryReference, UtxoContextBinding, UtxoEntryId, UtxoEntryReference, UtxoProcessor, UtxoSelectionContext,
};
use kaspa_rpc_core::GetUtxosByAddressesResponse;
use serde_wasm_bindgen::from_value;
use sorted_insert::SortedInsertBinaryByKey;
use workflow_wasm::abi::ref_from_abi;

static PROCESSOR_ID_SEQUENCER: AtomicU64 = AtomicU64::new(0);
fn next_processor_id() -> u64 {
    PROCESSOR_ID_SEQUENCER.fetch_add(1, Ordering::SeqCst)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct UtxoContextId(pub(crate) u64);

impl Default for UtxoContextId {
    fn default() -> Self {
        UtxoContextId(next_processor_id())
    }
}

impl From<AccountId> for UtxoContextId {
    fn from(id: AccountId) -> Self {
        UtxoContextId(id.0)
    }
}

impl From<&AccountId> for UtxoContextId {
    fn from(id: &AccountId) -> Self {
        UtxoContextId(id.0)
    }
}

impl UtxoContextId {
    pub fn new(id: u64) -> Self {
        UtxoContextId(id)
    }

    pub fn short(&self) -> String {
        let hex = self.to_hex();
        format!("[{}]", &hex[0..4])
    }
}

impl ToHex for UtxoContextId {
    fn to_hex(&self) -> String {
        format!("{:x}", self.0)
    }
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

impl From<(&UtxoEntryReference, &Instant)> for Consumed {
    fn from((entry, instant): (&UtxoEntryReference, &Instant)) -> Self {
        Self { entry: entry.clone(), instant: *instant }
    }
}

pub enum UtxoEntryVariant {
    Mature(UtxoEntryReference),
    Pending(UtxoEntryReference),
    Consumed(UtxoEntryReference),
}

#[derive(Default)]
pub struct Context {
    pub(crate) mature: Vec<UtxoEntryReference>,
    pub(crate) pending: HashMap<UtxoEntryId, UtxoEntryReference>,
    pub(crate) consumed: HashMap<UtxoEntryId, Consumed>,
    pub(crate) map: HashMap<UtxoEntryId, UtxoEntryReference>,
    outgoing: HashMap<TransactionId, PendingTransaction>,
    balance: Option<Balance>,
    addresses: Arc<DashSet<Arc<Address>>>,
}

impl Context {
    fn new_with_mature(mature: Vec<UtxoEntryReference>) -> Self {
        Self { mature, ..Default::default() }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.mature.clear();
        self.consumed.clear();
        self.pending.clear();
        self.outgoing.clear();
        self.addresses.clear();
        self.balance = None;
    }
}

struct Inner {
    id: UtxoContextId,
    binding: UtxoContextBinding,
    context: Mutex<Context>,
    processor: UtxoProcessor,
}

impl Inner {
    pub fn new(processor: &UtxoProcessor, binding: UtxoContextBinding) -> Self {
        Self { id: binding.id(), binding, context: Mutex::new(Context::default()), processor: processor.clone() }
    }

    pub fn new_with_mature_entries(processor: &UtxoProcessor, binding: UtxoContextBinding, mature: Vec<UtxoEntryReference>) -> Self {
        let context = Context::new_with_mature(mature);
        Self { id: binding.id(), binding, context: Mutex::new(context), processor: processor.clone() }
    }
}

/// a collection of UTXO entries
#[derive(Clone)]
#[wasm_bindgen]
pub struct UtxoContext {
    inner: Arc<Inner>,
}

#[wasm_bindgen]
impl UtxoContext {
    pub async fn clear(&self) -> Result<()> {
        let local = self.addresses();
        let addresses = local.iter().map(|v| v.clone()).collect::<Vec<_>>();
        if !addresses.is_empty() {
            self.processor().unregister_addresses(addresses).await?;
            local.clear();
        }

        self.context().clear();

        Ok(())
    }
}

impl UtxoContext {
    pub fn new(processor: &UtxoProcessor, binding: UtxoContextBinding) -> Self {
        Self { inner: Arc::new(Inner::new(processor, binding)) }
    }

    pub fn new_with_mature_entries(
        processor: &UtxoProcessor,
        binding: UtxoContextBinding,
        mature_entries: Vec<UtxoEntryReference>,
    ) -> Self {
        Self { inner: Arc::new(Inner::new_with_mature_entries(processor, binding, mature_entries)) }
    }

    pub fn context(&self) -> MutexGuard<Context> {
        self.inner.context.lock().unwrap()
    }

    pub fn processor(&self) -> &UtxoProcessor {
        &self.inner.processor
    }

    pub fn binding(&self) -> UtxoContextBinding {
        self.inner.binding.clone()
    }

    pub fn id(&self) -> UtxoContextId {
        self.inner.id
    }

    pub fn id_as_ref(&self) -> &UtxoContextId {
        &self.inner.id
    }

    pub fn mature_utxo_size(&self) -> usize {
        self.context().mature.len()
    }

    pub fn pending_utxo_size(&self) -> usize {
        self.context().pending.len()
    }

    pub fn balance(&self) -> Option<Balance> {
        self.context().balance.clone()
    }

    pub fn addresses(&self) -> Arc<DashSet<Arc<Address>>> {
        self.context().addresses.clone()
    }

    pub async fn update_balance(&self) -> Result<Balance> {
        let (balance, mature_utxo_size, pending_utxo_size) = {
            let previous_balance = self.balance();
            let mut balance = self.calculate_balance().await;
            balance.delta(&previous_balance);

            let mut context = self.context();
            context.balance.replace(balance.clone());
            let mature_utxo_size = context.mature.len();
            let pending_utxo_size = context.pending.len();

            (balance, mature_utxo_size, pending_utxo_size)
        };

        self.processor()
            .notify(Events::Balance { balance: Some(balance.clone()), id: self.id(), mature_utxo_size, pending_utxo_size })
            .await?;

        Ok(balance)
    }

    pub fn create_selection_context(&self) -> UtxoSelectionContext {
        UtxoSelectionContext::new(self)
    }

    /// Process pending transaction. Remove mature UTXO entries and add them to the consumed set.
    /// Produces a notification on the even multiplexer.
    pub(crate) async fn handle_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        {
            let mut context = self.context();
            let pending_utxo_entries = pending_tx.utxo_entries();
            context.mature.retain(|entry| !pending_utxo_entries.contains(entry));
            let now = Instant::now();
            pending_utxo_entries.iter().for_each(|entry| {
                context.consumed.insert(entry.id().clone(), (entry, &now).into());
            });

            context.outgoing.insert(pending_tx.id(), pending_tx.clone());
        }

        let record = TransactionRecord::new_outgoing(self, pending_tx);
        self.processor().notify(Events::Outgoing { record }).await?;

        Ok(())
    }

    /// Removes entries from mature utxo set and adds them to the consumed utxo set.
    /// NOTE: This method does not issue a notification on the event multiplexer.
    #[allow(dead_code)]
    pub(crate) async fn consume(&self, entries: &[UtxoEntryReference]) -> Result<()> {
        let mut context = self.context();
        context.mature.retain(|entry| !entries.contains(entry));
        let now = Instant::now();
        entries.iter().for_each(|entry| {
            context.consumed.insert(entry.id().clone(), (entry, &now).into());
        });

        Ok(())
    }

    /// Insert `utxo_entry` into the `UtxoSet`.
    /// NOTE: The insert will be ignored if already present in the inner map.
    pub async fn insert(&self, utxo_entry: UtxoEntryReference, current_daa_score: u64) -> Result<()> {
        let mut context = self.context();
        if let std::collections::hash_map::Entry::Vacant(e) = context.map.entry(utxo_entry.id().clone()) {
            e.insert(utxo_entry.clone());
            if utxo_entry.is_mature(current_daa_score) {
                context.mature.sorted_insert_binary_asc_by_key(utxo_entry, |entry| entry.amount_as_ref());
                Ok(())
            } else {
                context.pending.insert(utxo_entry.id().clone(), utxo_entry.clone());
                self.processor().pending().insert(utxo_entry.id().clone(), PendingUtxoEntryReference::new(utxo_entry, self.clone()));
                Ok(())
            }
        } else {
            log_error!("ignoring duplicate utxo entry");
            Ok(())
        }
    }

    pub async fn remove(&self, ids: Vec<UtxoEntryId>) -> Result<Vec<UtxoEntryVariant>> {
        let mut context = self.context();
        let mut removed = vec![];
        let mut remove_mature_ids = vec![];

        for id in ids.into_iter() {
            // remove from local map
            if context.map.remove(&id).is_some() {
                if let Some(pending) = context.pending.remove(&id) {
                    removed.push(UtxoEntryVariant::Pending(pending));
                    if self.processor().pending().remove(&id).is_none() {
                        log_error!("Error: unable to remove utxo entry from global pending (with context)");
                    }
                } else {
                    remove_mature_ids.push(id);
                }
            } else {
                log_error!("Error: unable to remove utxo entry from local map (with context)");
            }
        }

        let remove_mature_ids = remove_mature_ids
            .into_iter()
            .filter(|id| {
                // inner.consumed.remove(id).is_none()
                if let Some(consumed) = context.consumed.remove(id) {
                    removed.push(UtxoEntryVariant::Consumed(consumed.entry));
                    false
                } else {
                    true
                }
            })
            .collect::<Vec<_>>();

        context.mature.retain(|entry| {
            if remove_mature_ids.contains(&entry.id()) {
                // log_info!("REMOVE MATURE VIA REMOVED LIST: {}", entry.id());
                removed.push(UtxoEntryVariant::Mature(entry.clone()));
                false
            } else {
                // log_info!("MATURE NOT IN REMOVED LIST - RETAINING {}", entry.id());
                true
            }
        });

        Ok(removed)
    }

    pub async fn promote(&self, utxos: Vec<UtxoEntryReference>) -> Result<()> {
        let transactions = HashMap::group_from(utxos.iter().map(|utxo| (utxo.transaction_id(), utxo.clone())));

        for (txid, utxos) in transactions.into_iter() {
            for utxo_entry in utxos.iter() {
                let mut context = self.context();
                if context.pending.remove(utxo_entry.id_as_ref()).is_some() {
                    context.mature.sorted_insert_binary_asc_by_key(utxo_entry.clone(), |entry| entry.amount_as_ref());
                } else {
                    log_error!("Error: non-pending utxo promotion!");
                }
            }

            let is_outgoing = self.consume_outgoing_transaction(&txid);

            let record = TransactionRecord::new_incoming(self, TransactionType::Incoming, txid, utxos);
            self.processor().notify(Events::Maturity { record, is_outgoing }).await?;
        }

        Ok(())
    }

    fn is_outgoing_transaction(&self, txid: &TransactionId) -> bool {
        let context = self.context();
        context.outgoing.contains_key(txid)
    }

    fn consume_outgoing_transaction(&self, txid: &TransactionId) -> bool {
        let mut context = self.context();
        context.outgoing.remove(txid).is_some()
    }

    pub async fn extend(&self, utxo_entries: Vec<UtxoEntryReference>, current_daa_score: u64) -> Result<()> {
        let mut context = self.context();
        for utxo_entry in utxo_entries.into_iter() {
            if let std::collections::hash_map::Entry::Vacant(e) = context.map.entry(utxo_entry.id()) {
                e.insert(utxo_entry.clone());
                if utxo_entry.is_mature(current_daa_score) {
                    context.mature.push(utxo_entry);
                } else {
                    context.pending.insert(utxo_entry.id(), utxo_entry.clone());
                    self.processor().pending().insert(utxo_entry.id(), PendingUtxoEntryReference::new(utxo_entry, self.clone()));
                }
            } else {
                log_warning!("ignoring duplicate utxo entry");
            }
        }

        context.mature.sort();

        Ok(())
    }

    pub async fn chunks(&self, chunk_size: usize) -> Result<Vec<Vec<UtxoEntryReference>>> {
        let entries = &self.context().mature;
        let l = entries.chunks(chunk_size).map(|v| v.to_owned()).collect();
        Ok(l)
    }

    // recover UTXOs that went into `consumed` state but were never removed
    // from the set by the UtxoChanged notification.
    pub async fn recover(&self, duration: Option<Duration>) -> Result<()> {
        let checkpoint = Instant::now().checked_sub(duration.unwrap_or(Duration::from_secs(60))).unwrap();
        let mut context = self.context();
        let mut removed = vec![];
        context.consumed.retain(|_, consumed| {
            if consumed.instant < checkpoint {
                removed.push(consumed.entry.clone());
                false
            } else {
                true
            }
        });

        removed.into_iter().for_each(|entry| {
            context.mature.sorted_insert_binary_asc_by_key(entry, |entry| entry.amount_as_ref());
        });

        Ok(())
    }

    pub async fn calculate_balance(&self) -> Balance {
        let context = self.context();
        let mature = context.mature.iter().map(|e| e.as_ref().entry.amount).sum();
        let pending = context.pending.values().map(|e| e.as_ref().entry.amount).sum();
        Balance::new(mature, pending)
    }

    pub(crate) async fn handle_utxo_added(&self, utxos: Vec<UtxoEntryReference>) -> Result<()> {
        // add UTXOs to account set
        let current_daa_score = self.processor().current_daa_score().expect("daa score expected when invoking handle_utxo_added()");

        for utxo in utxos.iter() {
            if let Err(err) = self.insert(utxo.clone(), current_daa_score).await {
                log_error!("{}", err);
            }
        }

        let pending = HashMap::group_from(utxos.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));
        for (txid, utxos) in pending.into_iter() {
            let is_outgoing = self.is_outgoing_transaction(&txid);
            let record = TransactionRecord::new_incoming(self, TransactionType::Incoming, txid, utxos);
            self.processor().notify(Events::Pending { record, is_outgoing }).await?;
        }

        self.update_balance().await?;
        Ok(())
    }

    pub(crate) async fn handle_utxo_removed(&self, utxos: Vec<UtxoEntryReference>) -> Result<()> {
        // remove UTXOs from account set
        let utxo_ids: Vec<UtxoEntryId> = utxos.iter().map(|utxo| utxo.id()).collect();
        let removed = self.remove(utxo_ids).await?;

        let mut mature = vec![];
        let mut consumed = vec![];
        let mut pending = vec![];

        removed.into_iter().for_each(|entry| match entry {
            UtxoEntryVariant::Mature(utxo) => {
                mature.push(utxo);
            }
            UtxoEntryVariant::Consumed(utxo) => {
                consumed.push(utxo);
            }
            UtxoEntryVariant::Pending(utxo) => {
                pending.push(utxo);
            }
        });

        let mature = HashMap::group_from(mature.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));
        // let consumed = HashMap::group_from(consumed.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));
        let pending = HashMap::group_from(pending.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));

        for (txid, utxos) in mature.into_iter() {
            let record = TransactionRecord::new_external(self, txid, utxos);
            self.processor().notify(Events::External { record }).await?;
        }

        // for (txid, utxos) in consumed.into_iter() {
        //     let record = TransactionRecord::new_incoming(self, TransactionType::Debit, txid, utxos);
        //     self.processor().notify(Events::Debit { record }).await?;
        // }

        for (txid, utxos) in pending.into_iter() {
            let record = TransactionRecord::new_incoming(self, TransactionType::Reorg, txid, utxos);
            self.processor().notify(Events::Reorg { record }).await?;
        }

        // post balance update
        self.update_balance().await?;
        Ok(())
    }

    pub async fn register_addresses(&self, addresses: &[Address]) -> Result<()> {
        let local = self.addresses();

        let addresses = addresses
            .iter()
            .filter_map(|address| {
                let address = Arc::new(address.clone());
                if local.insert(address.clone()) {
                    Some(address)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        self.processor().register_addresses(addresses, self).await?;

        Ok(())
    }

    pub async fn unregister_addresses(self: &Arc<Self>, addresses: Vec<Address>) -> Result<()> {
        if !addresses.is_empty() {
            let local = self.addresses();
            let addresses = addresses.clone().into_iter().map(Arc::new).collect::<Vec<_>>();
            self.processor().unregister_addresses(addresses.clone()).await?;
            addresses.iter().for_each(|address| {
                local.remove(address);
            });
        } else {
            log_warning!("utxo processor: unregistering empty address set")
        }

        Ok(())
    }
}

#[wasm_bindgen]
impl UtxoContext {
    pub fn js_remove(&self, ids: Array) -> Result<Array> {
        let vec = ids.to_vec().iter().map(UtxoEntryId::try_from).collect::<Result<Vec<UtxoEntryId>>>()?;

        let mut context = self.context();

        let mut removed = vec![];
        for id in vec.iter() {
            if let Some(entry) = context.map.remove(id) {
                removed.push(entry)
            }
        }

        for entry in removed.iter() {
            if context.consumed.remove(&entry.id()).is_none() {
                context.mature.retain(|entry| entry.id() != entry.id());
            }
        }

        Ok(removed.into_iter().map(JsValue::from).collect::<Array>())
    }

    // #[wasm_bindgen(constructor)]
    // pub fn constructor(core: &JsValue, id_or_account : &JsValue, utxo_by_address_response: JsValue) -> Result<UtxoProcessor> {
    pub fn from(processor: &JsValue, id: u32, utxo_by_address_response: JsValue) -> Result<UtxoContext> {
        let processor = ref_from_abi!(UtxoProcessor, processor)?;
        let r: GetUtxosByAddressesResponse = from_value(utxo_by_address_response)?;
        let mut entries = r.entries.into_iter().map(|entry| entry.into()).collect::<Vec<UtxoEntryReference>>();
        entries.sort_by_key(|e| e.amount());
        let binding = UtxoContextBinding::Id(UtxoContextId::new(id as u64));
        let utxo_context = UtxoContext::new_with_mature_entries(&processor, binding, entries);
        Ok(utxo_context)
    }

    #[wasm_bindgen(js_name=calculateBalance)]
    pub async fn js_calculate_balance(&self) -> crate::wasm::wallet::Balance {
        self.calculate_balance().await.into()
    }

    #[wasm_bindgen(js_name=createSelectionContext)]
    pub fn js_create_selection_context(&self) -> UtxoSelectionContext {
        UtxoSelectionContext::new(self)
    }
}

impl Eq for UtxoContext {}

impl PartialEq for UtxoContext {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl std::hash::Hash for UtxoContext {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl Ord for UtxoContext {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().cmp(other.id_as_ref())
    }
}

impl PartialOrd for UtxoContext {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.id().cmp(other.id_as_ref()))
    }
}
