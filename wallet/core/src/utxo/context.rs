use crate::encryption::sha256_hash;
use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{AccountId, Balance};
use crate::storage::{TransactionRecord, TransactionType};
use crate::tx::PendingTransaction;
use crate::utxo::{
    PendingUtxoEntryReference, UtxoContextBinding, UtxoEntryId, UtxoEntryReference, UtxoEntryReferenceExtension, UtxoProcessor,
};
use kaspa_hashes::Hash;
use sorted_insert::SortedInsertBinaryByKey;

static PROCESSOR_ID_SEQUENCER: AtomicU64 = AtomicU64::new(0);
fn next_processor_id() -> Hash {
    let id = PROCESSOR_ID_SEQUENCER.fetch_add(1, Ordering::SeqCst);
    Hash::from_slice(sha256_hash(id.to_le_bytes().as_slice()).as_ref())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct UtxoContextId(pub(crate) Hash);

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
    pub fn new(id: Hash) -> Self {
        UtxoContextId(id)
    }

    pub fn short(&self) -> String {
        let hex = self.to_hex();
        format!("[{}]", &hex[0..4])
    }
}

impl ToHex for UtxoContextId {
    fn to_hex(&self) -> String {
        self.0.to_hex()
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
    /// Mature (Confirmed) UTXOs
    pub(crate) mature: Vec<UtxoEntryReference>,
    /// UTXOs that are pending confirmation
    pub(crate) pending: HashMap<UtxoEntryId, UtxoEntryReference>,
    /// UTXOs consumed by recently created outgoing transactions
    pub(crate) consumed: HashMap<UtxoEntryId, Consumed>,
    /// All UTXOs in posession of this context instance
    pub(crate) map: HashMap<UtxoEntryId, UtxoEntryReference>,
    /// Outgoing transactions that have not yet been confirmed.
    /// Confirmation occurs when the transaction UTXOs are
    /// removed from the context by the UTXO change notification.
    outgoing: HashMap<TransactionId, PendingTransaction>,
    /// Total balance of all UTXOs in this context (mature, pending)
    balance: Option<Balance>,
    /// Addresses monitored by this UTXO context
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
pub struct UtxoContext {
    inner: Arc<Inner>,
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

        let processor = self.processor();
        processor.register_recoverable_context(self);
        let record = TransactionRecord::new_outgoing(self, pending_tx);
        processor.notify(Events::Outgoing { record }).await?;

        Ok(())
    }

    /// Removes entries from mature utxo set and adds them to the consumed utxo set.
    /// NOTE: This method does not issue a notification on the event multiplexer.
    /// This has been replaced with `handle_outgoing_transaction`, pending decision
    /// on removal.
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
                // log_info!("remove mature via removed list: {}", entry.id());
                removed.push(UtxoEntryVariant::Mature(entry.clone()));
                false
            } else {
                // log_info!("mature not in removed list - retaining {}", entry.id());
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

    /// recover UTXOs that went into `consumed` state but were never removed
    /// from the set by the UtxoChanged notification.
    pub fn recover(&self, _current_daa_score: u64, duration: Option<Duration>) -> bool {
        let mut context = self.context();
        if context.consumed.is_empty() {
            return false;
        }

        let checkpoint = Instant::now()
            .checked_sub(
                duration.unwrap_or_else(|| Duration::from_secs(crate::utxo::UTXO_RECOVERY_PERIOD_SECONDS.load(Ordering::Relaxed))),
            )
            .expect("UtxoContext::recover() invalid recovery period");

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

        context.consumed.is_not_empty()
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
        let pending = HashMap::group_from(pending.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));

        for (txid, utxos) in mature.into_iter() {
            let record = TransactionRecord::new_external(self, txid, utxos);
            self.processor().notify(Events::External { record }).await?;
        }

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

    pub async fn unregister_addresses(&self, addresses: Vec<Address>) -> Result<()> {
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

    pub async fn scan_and_register_addresses(&self, addresses: Vec<Address>, current_daa_score: Option<u64>) -> Result<()> {
        self.register_addresses(&addresses).await?;
        let resp = self.processor().rpc_api().get_utxos_by_addresses(addresses).await?;
        let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();
        let current_daa_score = current_daa_score.unwrap_or_else(|| {
            self.processor()
                .current_daa_score()
                .expect("daa score or initialized UtxoProcessor are when invoking scan_and_register_addresses()")
        });
        self.extend(refs, current_daa_score).await?;
        self.update_balance().await?;
        Ok(())
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
