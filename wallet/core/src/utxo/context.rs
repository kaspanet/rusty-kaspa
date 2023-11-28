use crate::encryption::sha256_hash;
use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{AccountId, Balance};
use crate::storage::{TransactionRecord, TransactionType};
use crate::tx::PendingTransaction;
use crate::utxo::{
    Maturity, PendingUtxoEntryReference, UtxoContextBinding, UtxoEntryId, UtxoEntryReference, UtxoEntryReferenceExtension,
    UtxoProcessor,
};
use kaspa_hashes::Hash;
use sorted_insert::SortedInsertBinaryByKey;

// If enabled, upon submission of an outgoing transaction,
// change UTXOs are immediately promoted to the mature set.
// Otherwise they are treated as regular incoming transactions
// and require a maturity period.
// const SKIP_CHANGE_UTXO_PROMOTION: bool = true;

static PROCESSOR_ID_SEQUENCER: AtomicU64 = AtomicU64::new(0);
fn next_processor_id() -> Hash {
    let id = PROCESSOR_ID_SEQUENCER.fetch_add(1, Ordering::SeqCst);
    Hash::from_slice(sha256_hash(id.to_le_bytes().as_slice()).as_ref())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
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

impl From<UtxoContextId> for AccountId {
    fn from(id: UtxoContextId) -> Self {
        AccountId(id.0)
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

impl std::fmt::Display for UtxoContextId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct Consumed {
    entry: UtxoEntryReference,
    // timeout: Instant,
    timeout_daa: Daa,
}

impl Consumed {
    pub fn amount(&self) -> u64 {
        self.entry.amount()
    }
}

impl From<(UtxoEntryReference, &Daa)> for Consumed {
    fn from((entry, timeout_daa): (UtxoEntryReference, &Daa)) -> Self {
        Self { entry, timeout_daa: *timeout_daa }
    }
}

impl From<(&UtxoEntryReference, &Daa)> for Consumed {
    fn from((entry, timeout_daa): (&UtxoEntryReference, &Daa)) -> Self {
        Self { entry: entry.clone(), timeout_daa: *timeout_daa }
    }
}

// impl From<(UtxoEntryReference, &Instant)> for Consumed {
//     fn from((entry, timeout, timeout_daa): (UtxoEntryReference, &Instant, &Daa)) -> Self {
//         Self { entry, timeout: *timeout, timeout_daa: *timeout_daa }
//     }
// }

// impl From<(&UtxoEntryReference, &Instant)> for Consumed {
//     fn from((entry, timeout): (&UtxoEntryReference, &Instant)) -> Self {
//         Self { entry: entry.clone(), timeout: *timeout }
//     }
// }

pub enum UtxoEntryVariant {
    Mature(UtxoEntryReference),
    Pending(UtxoEntryReference),
    Stasis(UtxoEntryReference),
    Consumed(UtxoEntryReference),
}

pub struct Context {
    /// Mature (Confirmed) UTXOs
    pub(crate) mature: Vec<UtxoEntryReference>,
    /// UTXOs that are pending confirmation
    pub(crate) pending: HashMap<UtxoEntryId, UtxoEntryReference>,
    /// UTXOs that are in stasis (freshly minted coinbase transactions only)
    pub(crate) stasis: HashMap<UtxoEntryId, UtxoEntryReference>,
    /// UTXOs consumed by recently created outgoing transactions
    pub(crate) consumed: HashMap<UtxoEntryId, Consumed>,
    /// All UTXOs in possession of this context instance
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

impl Default for Context {
    fn default() -> Self {
        Self {
            mature: vec![],
            pending: HashMap::default(),
            stasis: HashMap::default(),
            consumed: HashMap::default(),
            map: HashMap::default(),
            outgoing: HashMap::default(),
            balance: None,
            addresses: Arc::new(DashSet::new()),
            // recovery_period: Duration::from_secs(crate::utxo::UTXO_RECOVERY_PERIOD_SECONDS.load(Ordering::Relaxed)),
        }
    }
}

impl Context {
    fn new_with_mature(mature: Vec<UtxoEntryReference>) -> Self {
        Self { mature, ..Default::default() }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.mature.clear();
        self.stasis.clear();
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
    // /// Timeout for UTXO recovery
    // recovery_period: Duration,
    recovery_period: Daa,
}

impl Inner {
    pub fn new(processor: &UtxoProcessor, binding: UtxoContextBinding) -> Self {
        Self {
            id: binding.id(),
            binding,
            context: Mutex::new(Context::default()),
            processor: processor.clone(),
            recovery_period: Daa(crate::utxo::UTXO_RECOVERY_PERIOD_DAA.load(Ordering::Relaxed)),
        }
    }

    pub fn new_with_mature_entries(processor: &UtxoProcessor, binding: UtxoContextBinding, mature: Vec<UtxoEntryReference>) -> Self {
        let context = Context::new_with_mature(mature);
        Self {
            id: binding.id(),
            binding,
            context: Mutex::new(context),
            processor: processor.clone(),
            recovery_period: Daa(crate::utxo::UTXO_RECOVERY_PERIOD_DAA.load(Ordering::Relaxed)),
        }
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
    pub(crate) async fn register_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        {
            let current_daa_score =
                self.processor().current_daa_score().expect("daa score expected when invoking register_outgoing_transaction()");

            let mut context = self.context();
            let pending_utxo_entries = pending_tx.utxo_entries();
            context.mature.retain(|entry| !pending_utxo_entries.contains(entry));

            let timeout_daa = Daa(current_daa_score + self.inner.recovery_period.0);
            pending_utxo_entries.iter().for_each(|entry| {
                context.consumed.insert(entry.id().clone(), (entry, &timeout_daa).into());
            });

            context.outgoing.insert(pending_tx.id(), pending_tx.clone());
        }

        self.processor().register_recoverable_context(self);

        Ok(())
    }

    pub(crate) async fn notify_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        let record = TransactionRecord::new_outgoing(self, pending_tx);
        self.processor().notify(Events::Outgoing { record }).await?;
        self.update_balance().await?;
        Ok(())
    }

    pub(crate) async fn cancel_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        let mut context = self.context();
        let pending_utxo_entries = pending_tx.utxo_entries();

        context.outgoing.retain(|id, _| id != &pending_tx.id());
        context.consumed.retain(|_, consumed| !pending_utxo_entries.contains(&consumed.entry));
        pending_utxo_entries.iter().for_each(|entry| {
            context.mature.push(entry.clone());
        });

        Ok(())
    }

    /// Insert `utxo_entry` into the `UtxoSet`.
    /// NOTE: The insert will be ignored if already present in the inner map.
    pub async fn insert(&self, utxo_entry: UtxoEntryReference, current_daa_score: u64, force_maturity: bool) -> Result<()> {
        let mut context = self.context();
        if let std::collections::hash_map::Entry::Vacant(e) = context.map.entry(utxo_entry.id().clone()) {
            e.insert(utxo_entry.clone());
            if force_maturity {
                context.mature.sorted_insert_binary_asc_by_key(utxo_entry.clone(), |entry| entry.amount_as_ref());
            } else {
                match utxo_entry.maturity(current_daa_score) {
                    Maturity::Stasis => {
                        context.stasis.insert(utxo_entry.id().clone(), utxo_entry.clone());
                        self.processor()
                            .stasis()
                            .insert(utxo_entry.id().clone(), PendingUtxoEntryReference::new(utxo_entry, self.clone()));
                    }
                    Maturity::Pending => {
                        context.pending.insert(utxo_entry.id().clone(), utxo_entry.clone());
                        self.processor()
                            .pending()
                            .insert(utxo_entry.id().clone(), PendingUtxoEntryReference::new(utxo_entry, self.clone()));
                    }
                    Maturity::Mature => {
                        context.mature.sorted_insert_binary_asc_by_key(utxo_entry.clone(), |entry| entry.amount_as_ref());
                    }
                }
            }
            Ok(())
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
                } else if let Some(stasis) = context.stasis.remove(&id) {
                    removed.push(UtxoEntryVariant::Stasis(stasis));
                    if self.processor().stasis().remove(&id).is_none() {
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
                removed.push(UtxoEntryVariant::Mature(entry.clone()));
                false
            } else {
                true
            }
        });

        Ok(removed)
    }

    /// This function handles `Pending` to `Mature` transformation.
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

            let outgoing_transaction = self.consume_outgoing_transaction(&txid);
            if let Some(transaction) = outgoing_transaction {
                let record = TransactionRecord::new_outgoing(self, &transaction);
                self.processor().notify(Events::Maturity { record }).await?;
            } else {
                let record = TransactionRecord::new_incoming(self, TransactionType::Incoming, txid, utxos);
                self.processor().notify(Events::Maturity { record }).await?;
            }
        }

        Ok(())
    }

    /// This function handles `Stasis` to `Pending` transformation.
    pub async fn revive(&self, utxos: Vec<UtxoEntryReference>) -> Result<()> {
        let transactions = HashMap::group_from(utxos.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));

        for (txid, utxos) in transactions.into_iter() {
            for utxo_entry in utxos.iter() {
                let mut context = self.context();
                if context.stasis.remove(utxo_entry.id_as_ref()).is_some() {
                    context.pending.insert(utxo_entry.id(), utxo_entry.clone());
                }
                if context.pending.remove(utxo_entry.id_as_ref()).is_some() {
                    context.mature.sorted_insert_binary_asc_by_key(utxo_entry.clone(), |entry| entry.amount_as_ref());
                } else {
                    log_error!("Error: non-pending utxo promotion!");
                }
            }

            let record = TransactionRecord::new_incoming(self, TransactionType::Incoming, txid, utxos);
            self.processor().notify(Events::Pending { record }).await?;
        }

        Ok(())
    }

    /// Obtain a clone of the outgoing transaction with the given `txid` from the outgoing tx list.
    fn get_outgoing_transaction(&self, txid: &TransactionId) -> Option<PendingTransaction> {
        let context = self.context();
        context.outgoing.get(txid).cloned()
    }

    /// Consume (remove) the outgoing transaction with the given `txid` from the outgoing tx list.
    fn consume_outgoing_transaction(&self, txid: &TransactionId) -> Option<PendingTransaction> {
        let mut context = self.context();
        context.outgoing.remove(txid)
    }

    pub async fn extend(&self, utxo_entries: Vec<UtxoEntryReference>, current_daa_score: u64) -> Result<()> {
        let mut context = self.context();
        for utxo_entry in utxo_entries.into_iter() {
            if let std::collections::hash_map::Entry::Vacant(e) = context.map.entry(utxo_entry.id()) {
                e.insert(utxo_entry.clone());
                match utxo_entry.maturity(current_daa_score) {
                    Maturity::Stasis => {
                        context.stasis.insert(utxo_entry.id().clone(), utxo_entry.clone());
                        self.processor()
                            .stasis()
                            .insert(utxo_entry.id().clone(), PendingUtxoEntryReference::new(utxo_entry, self.clone()));
                    }
                    Maturity::Pending => {
                        context.pending.insert(utxo_entry.id().clone(), utxo_entry.clone());
                        self.processor()
                            .pending()
                            .insert(utxo_entry.id().clone(), PendingUtxoEntryReference::new(utxo_entry, self.clone()));
                    }
                    Maturity::Mature => {
                        context.mature.sorted_insert_binary_asc_by_key(utxo_entry.clone(), |entry| entry.amount_as_ref());
                    }
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
    pub fn recover(&self, current_daa_score: u64) -> bool {
        let mut context = self.context();
        if context.consumed.is_empty() {
            return false;
        }

        // let now = Instant::now();
        let mut removed = vec![];
        context.consumed.retain(|_, consumed| {
            if current_daa_score > consumed.timeout_daa.0 {
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
        let consumed: u64 = context.consumed.values().map(|e| e.entry.amount()).sum();
        let mature: u64 = context.mature.iter().map(|e| e.as_ref().entry.amount).sum();
        let pending: u64 = context.pending.values().map(|e| e.as_ref().entry.amount).sum();
        // this will aggregate only transactions containing final payments (not compound transactions)
        let outgoing = context.outgoing.values().filter_map(|tx| tx.payment_value().map(|value| value + tx.fees())).sum::<u64>(); //.collect::<Vec<_>>();
        Balance::new((mature + consumed) - outgoing, pending, outgoing)
    }

    pub(crate) async fn handle_utxo_added(&self, utxos: Vec<UtxoEntryReference>, current_daa_score: u64) -> Result<()> {
        // add UTXOs to account set

        // If SKIP_CHANGE_UTXO_PROMOTION is enabled, we consume outgoing transactions now
        // otherwise the process is deferred to the promotion time.
        let mut discarded_outgoing_transactions = HashSet::new();

        let added = HashMap::group_from(utxos.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));
        for (txid, utxos) in added.into_iter() {
            let outgoing_transaction = self.get_outgoing_transaction(&txid);
            let force_maturity_if_outgoing = outgoing_transaction.is_some(); // && self.inner.skip_change_utxo_promotion;
            let is_coinbase_stasis =
                utxos.first().map(|utxo| matches!(utxo.maturity(current_daa_score), Maturity::Stasis)).unwrap_or_default();

            for utxo in utxos.iter() {
                if let Err(err) = self.insert(utxo.clone(), current_daa_score, force_maturity_if_outgoing).await {
                    log_error!("{}", err);
                }
            }

            if let Some(transaction) = outgoing_transaction {
                    discarded_outgoing_transactions.insert(txid);
                    let record = TransactionRecord::new_outgoing(self, &transaction);
                    self.processor().notify(Events::Change { record }).await?;
            } else if !is_coinbase_stasis {
                // do not notify if coinbase transaction is in stasis
                let record = TransactionRecord::new_incoming(self, TransactionType::Incoming, txid, utxos);
                self.processor().notify(Events::Pending { record }).await?;
            }
        }

        for txid in discarded_outgoing_transactions.into_iter() {
            self.consume_outgoing_transaction(&txid);
        }

        Ok(())
    }

    pub(crate) async fn handle_utxo_removed(&self, utxos: Vec<UtxoEntryReference>, _current_daa_score: u64) -> Result<()> {
        // remove UTXOs from account set
        let utxo_ids: Vec<UtxoEntryId> = utxos.iter().map(|utxo| utxo.id()).collect();
        let removed = self.remove(utxo_ids).await?;

        let mut mature = vec![];
        let mut consumed = vec![];
        let mut pending = vec![];
        let mut stasis = vec![];

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
            UtxoEntryVariant::Stasis(utxo) => {
                stasis.push(utxo);
            }
        });

        let mature = HashMap::group_from(mature.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));
        let pending = HashMap::group_from(pending.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));
        let stasis = HashMap::group_from(stasis.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));

        for (txid, utxos) in mature.into_iter() {
            let record = TransactionRecord::new_external(self, txid, utxos);
            self.processor().notify(Events::External { record }).await?;
        }

        for (txid, utxos) in pending.into_iter() {
            let record = TransactionRecord::new_incoming(self, TransactionType::Reorg, txid, utxos);
            self.processor().notify(Events::Reorg { record }).await?;
        }

        for (txid, utxos) in stasis.into_iter() {
            let record = TransactionRecord::new_incoming(self, TransactionType::Stasis, txid, utxos);
            self.processor().notify(Events::Stasis { record }).await?;
        }

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
