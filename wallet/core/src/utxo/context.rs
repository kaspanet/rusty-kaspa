//!
//! Implementation of the [`UtxoContext`] which is a runtime
//! primitive responsible for monitoring multiple addresses,
//! generation of address-related events and balance tracking.
//!

use crate::encryption::sha256_hash;
use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::storage::TransactionRecord;
use crate::tx::PendingTransaction;
use crate::utxo::{
    Maturity, NetworkParams, OutgoingTransaction, PendingUtxoEntryReference, UtxoContextBinding, UtxoEntryId, UtxoEntryReference,
    UtxoEntryReferenceExtension, UtxoProcessor,
};
use kaspa_hashes::Hash;
use sorted_insert::SortedInsertBinaryByKey;

static UTXO_CONTEXT_ID_SEQUENCER: AtomicU64 = AtomicU64::new(0);
fn next_utxo_context_id() -> Hash {
    let id = UTXO_CONTEXT_ID_SEQUENCER.fetch_add(1, Ordering::SeqCst);
    Hash::from_slice(sha256_hash(id.to_le_bytes().as_slice()).as_ref())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct UtxoContextId(pub(crate) Hash);

impl Default for UtxoContextId {
    fn default() -> Self {
        UtxoContextId(next_utxo_context_id())
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

pub enum UtxoEntryVariant {
    Mature(UtxoEntryReference),
    Pending(UtxoEntryReference),
    Stasis(UtxoEntryReference),
}

pub struct Context {
    /// Mature (Confirmed) UTXOs
    pub(crate) mature: Vec<UtxoEntryReference>,
    /// UTXOs that are pending confirmation
    pub(crate) pending: AHashMap<UtxoEntryId, UtxoEntryReference>,
    /// UTXOs that are in stasis (freshly minted coinbase transactions only)
    pub(crate) stasis: AHashMap<UtxoEntryId, UtxoEntryReference>,
    /// All UTXOs in possession of this context instance
    pub(crate) map: AHashMap<UtxoEntryId, UtxoEntryReference>,
    /// Outgoing transactions that have not yet been confirmed.
    /// Confirmation occurs when the transaction UTXOs are
    /// removed from the context by the UTXO change notification.
    pub(crate) outgoing: AHashMap<TransactionId, OutgoingTransaction>,
    /// Total balance of all UTXOs in this context (mature, pending)
    balance: Option<Balance>,
    /// Addresses monitored by this UTXO context
    addresses: Arc<DashSet<Arc<Address>>>,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            mature: vec![],
            pending: AHashMap::default(),
            stasis: AHashMap::default(),
            map: AHashMap::default(),
            outgoing: AHashMap::default(),
            balance: None,
            addresses: Arc::new(DashSet::new()),
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

///
///  UtxoContext is a data structure responsible for monitoring multiple addresses
/// for transactions.  It scans the address set for existing UtxoEntry records, then
/// monitors for transaction-related events in order to maintain a consistent view
/// on that UtxoEntry set throughout its connection lifetime.
///
/// UtxoContext typically represents a single wallet account, but can monitor any set
/// of addresses. When receiving transaction events, UtxoContext detects types of these
/// events and emits corresponding notifications on the UtxoProcessor event multiplexer.
///
/// In addition to standard monitoring, UtxoContext works in conjunction with the
/// TransactionGenerator to track outgoing transactions in an effort to segregate
/// different types of UtxoEntry updates (regular incoming vs. change).
///
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
        let balance = {
            let previous_balance = self.balance();
            let mut balance = self.calculate_balance().await;
            balance.delta(&previous_balance);
            let mut context = self.context();
            context.balance.replace(balance.clone());
            balance
        };
        self.processor().notify(Events::Balance { balance: Some(balance.clone()), id: self.id() }).await?;

        Ok(balance)
    }

    /// Process pending transaction. Remove mature UTXO entries and add them to the consumed set.
    /// Produces a notification on the even multiplexer.
    pub(crate) async fn register_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        {
            let current_daa_score =
                self.processor().current_daa_score().ok_or(Error::MissingDaaScore("register_outgoing_transaction()"))?;

            let mut context = self.context();
            let pending_utxo_entries = pending_tx.utxo_entries();
            context.mature.retain(|entry| !pending_utxo_entries.contains_key(&entry.id()));

            let outgoing_transaction = OutgoingTransaction::new(current_daa_score, self.clone(), pending_tx.clone());
            self.processor().register_outgoing_transaction(outgoing_transaction.clone());
            context.outgoing.insert(outgoing_transaction.id(), outgoing_transaction);
        }

        Ok(())
    }

    pub(crate) async fn notify_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        let outgoing_tx = self.processor().outgoing().get(&pending_tx.id()).expect("outgoing transaction for notification");

        if pending_tx.is_batch() {
            let record = TransactionRecord::new_batch(self, &outgoing_tx, None)?;
            self.processor().notify(Events::Pending { record }).await?;
        } else {
            let record = TransactionRecord::new_outgoing(self, &outgoing_tx, None)?;
            self.processor().notify(Events::Pending { record }).await?;
        }
        self.update_balance().await?;
        Ok(())
    }

    /// Cancel outgoing transaction in case of a submission error. Removes [`OutgoingTransaction`] from the
    /// [`UtxoProcessor`] and returns UtxoEntries from the outgoing transaction back to the mature pool.
    pub(crate) async fn cancel_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        self.processor().cancel_outgoing_transaction(pending_tx.id());

        let mut context = self.context();

        let outgoing_transaction = context.outgoing.remove(&pending_tx.id()).expect("outgoing transaction");
        outgoing_transaction.utxo_entries().iter().for_each(|(_, entry)| {
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
                let params = NetworkParams::from(self.processor().network_id()?);
                match utxo_entry.maturity(&params, current_daa_score) {
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
                    Maturity::Confirmed => {
                        context.mature.sorted_insert_binary_asc_by_key(utxo_entry.clone(), |entry| entry.amount_as_ref());
                    }
                }
            }
            Ok(())
        } else {
            log_warn!("ignoring duplicate utxo entry");
            Ok(())
        }
    }

    pub async fn remove(&self, utxos: Vec<UtxoEntryReference>) -> Result<Vec<UtxoEntryVariant>> {
        let mut context = self.context();
        let mut removed = vec![];
        let mut remove_mature_ids = vec![];

        for utxo in utxos.into_iter() {
            let id = utxo.id();
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
                log_error!("Error: UTXO not found in UtxoContext map!");
            }
        }

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
                    unreachable!("Error: non-pending utxo promotion!");
                }
            }

            if self.context().outgoing.get(&txid).is_some() {
                unreachable!("Error: promotion of the outgoing transaction!");
            }

            let record = TransactionRecord::new_incoming(self, txid, &utxos);
            self.processor().notify(Events::Maturity { record }).await?;
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
                } else {
                    log_error!("Error: non-stasis utxo revival!");
                    panic!("Error: non-stasis utxo revival!");
                }
            }

            let record = TransactionRecord::new_incoming(self, txid, &utxos);
            self.processor().notify(Events::Pending { record }).await?;
        }

        Ok(())
    }

    pub fn remove_outgoing_transaction(&self, txid: &TransactionId) -> Option<OutgoingTransaction> {
        let mut context = self.context();
        context.outgoing.remove(txid)
    }

    pub async fn extend_from_scan(&self, utxo_entries: Vec<UtxoEntryReference>, current_daa_score: u64) -> Result<()> {
        let (pending, mature) = {
            let mut context = self.context();

            let mut pending = vec![];
            let mut mature = vec![];

            let params = NetworkParams::from(self.processor().network_id()?);

            for utxo_entry in utxo_entries.into_iter() {
                if let std::collections::hash_map::Entry::Vacant(e) = context.map.entry(utxo_entry.id()) {
                    e.insert(utxo_entry.clone());
                    match utxo_entry.maturity(&params, current_daa_score) {
                        Maturity::Stasis => {
                            context.stasis.insert(utxo_entry.id().clone(), utxo_entry.clone());
                            self.processor()
                                .stasis()
                                .insert(utxo_entry.id().clone(), PendingUtxoEntryReference::new(utxo_entry, self.clone()));
                        }
                        Maturity::Pending => {
                            pending.push(utxo_entry.clone());
                            context.pending.insert(utxo_entry.id().clone(), utxo_entry.clone());
                            self.processor()
                                .pending()
                                .insert(utxo_entry.id().clone(), PendingUtxoEntryReference::new(utxo_entry, self.clone()));
                        }
                        Maturity::Confirmed => {
                            mature.push(utxo_entry.clone());
                            context.mature.sorted_insert_binary_asc_by_key(utxo_entry.clone(), |entry| entry.amount_as_ref());
                        }
                    }
                } else {
                    log_warn!("ignoring duplicate utxo entry");
                }
            }

            (pending, mature)
        };

        // cascade discovery to the processor
        // for unixtime resolution

        let pending = HashMap::group_from(pending.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));
        for (id, utxos) in pending.into_iter() {
            let record = TransactionRecord::new_external(self, id, &utxos);
            self.processor().handle_discovery(record).await?;
        }

        let mature = HashMap::group_from(mature.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));
        for (id, utxos) in mature.into_iter() {
            let record = TransactionRecord::new_external(self, id, &utxos);
            self.processor().handle_discovery(record).await?;
        }

        Ok(())
    }

    pub async fn calculate_balance(&self) -> Balance {
        let context = self.context();
        let mature: u64 = context.mature.iter().map(|e| e.as_ref().amount).sum();
        let pending: u64 = context.pending.values().map(|e| e.as_ref().amount).sum();

        // this will aggregate only transactions containing
        // the final payments (not compound transactions)
        // and outgoing transactions that have not yet
        // been accepted
        let mut outgoing: u64 = 0;
        let mut consumed: u64 = 0;
        for tx in context.outgoing.values() {
            if !tx.is_accepted() {
                if let Some(payment_value) = tx.payment_value() {
                    // final tx
                    outgoing += tx.fees() + payment_value;
                    consumed += tx.aggregate_input_value();
                } else {
                    // compound tx has no payment value
                    outgoing += tx.fees() + tx.aggregate_output_value();
                    consumed += tx.aggregate_input_value()
                }
            }
        }

        // TODO - remove this check once we are confident that
        // this condition does not occur. This is a temporary
        // log for a fixed bug, but we want to keep the check
        // just in case.
        if mature + consumed < outgoing {
            log_error!("Error: outgoing transaction value exceeds available balance");
        }

        let mature = (mature + consumed).saturating_sub(outgoing);

        Balance::new(mature, pending, outgoing, context.mature.len(), context.pending.len(), context.stasis.len())
    }

    pub(crate) async fn handle_utxo_added(&self, utxos: Vec<UtxoEntryReference>, current_daa_score: u64) -> Result<()> {
        // add UTXOs to account set

        let params = NetworkParams::from(self.processor().network_id()?);

        let mut accepted_outgoing_transactions = AHashSet::new();

        let added = HashMap::group_from(utxos.into_iter().map(|utxo| (utxo.transaction_id(), utxo)));
        for (txid, utxos) in added.into_iter() {
            // get outgoing transaction from the processor in case the transaction
            // originates from a different [`Account`] represented by a different [`UtxoContext`].
            let outgoing_transaction = self.processor().outgoing().get(&txid);

            let force_maturity_if_outgoing = outgoing_transaction.is_some();
            let is_coinbase_stasis =
                utxos.first().map(|utxo| matches!(utxo.maturity(&params, current_daa_score), Maturity::Stasis)).unwrap_or_default();

            for utxo in utxos.iter() {
                if let Err(err) = self.insert(utxo.clone(), current_daa_score, force_maturity_if_outgoing).await {
                    // TODO - remove `Result<>` from insert at a later date once
                    // we are confident that the insert will never result in an error.
                    log_error!("{}", err);
                }
            }

            if let Some(outgoing_transaction) = outgoing_transaction {
                accepted_outgoing_transactions.insert((*outgoing_transaction).clone());

                if outgoing_transaction.is_batch() {
                    let record = TransactionRecord::new_batch(self, &outgoing_transaction, Some(current_daa_score))?;
                    self.processor().notify(Events::Maturity { record }).await?;
                } else if outgoing_transaction.originating_context() == self {
                    let record = TransactionRecord::new_change(self, &outgoing_transaction, Some(current_daa_score), &utxos)?;
                    self.processor().notify(Events::Maturity { record }).await?;
                } else {
                    let record =
                        TransactionRecord::new_transfer_incoming(self, &outgoing_transaction, Some(current_daa_score), &utxos)?;
                    self.processor().notify(Events::Maturity { record }).await?;
                }
            } else if !is_coinbase_stasis {
                // do not notify if coinbase transaction is in stasis
                let record = TransactionRecord::new_incoming(self, txid, &utxos);
                self.processor().notify(Events::Pending { record }).await?;
            }
        }

        for outgoing_transaction in accepted_outgoing_transactions.into_iter() {
            outgoing_transaction.tag_as_accepted_at_daa_score(current_daa_score);
        }

        Ok(())
    }

    pub(crate) async fn handle_utxo_removed(&self, mut utxos: Vec<UtxoEntryReference>, current_daa_score: u64) -> Result<()> {
        // remove UTXOs from account set

        let outgoing_transactions = self.processor().outgoing();
        let mut accepted_outgoing_transactions = HashSet::<OutgoingTransaction>::new();

        utxos.retain(|utxo| {
            for outgoing_transaction in outgoing_transactions.iter() {
                if outgoing_transaction.utxo_entries().contains_key(&utxo.id()) {
                    accepted_outgoing_transactions.insert((*outgoing_transaction).clone());
                    return false;
                }
            }
            true
        });

        for accepted_outgoing_transaction in accepted_outgoing_transactions.into_iter() {
            if accepted_outgoing_transaction.is_batch() {
                let record = TransactionRecord::new_batch(self, &accepted_outgoing_transaction, Some(current_daa_score))?;
                self.processor().notify(Events::Maturity { record }).await?;
            } else if accepted_outgoing_transaction.destination_context().is_some() {
                let record =
                    TransactionRecord::new_transfer_outgoing(self, &accepted_outgoing_transaction, Some(current_daa_score), &utxos)?;
                self.processor().notify(Events::Maturity { record }).await?;
            } else {
                let record = TransactionRecord::new_outgoing(self, &accepted_outgoing_transaction, Some(current_daa_score))?;
                self.processor().notify(Events::Maturity { record }).await?;
            }
        }

        if utxos.is_empty() {
            return Ok(());
        }

        let removed = self.remove(utxos).await?;

        let mut mature = vec![];
        let mut pending = vec![];
        let mut stasis = vec![];

        removed.into_iter().for_each(|entry| match entry {
            UtxoEntryVariant::Mature(utxo) => {
                mature.push(utxo);
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
            let record = TransactionRecord::new_external(self, txid, &utxos);
            self.processor().notify(Events::Maturity { record }).await?;
        }

        for (txid, utxos) in pending.into_iter() {
            let record = TransactionRecord::new_reorg(self, txid, &utxos);
            self.processor().notify(Events::Reorg { record }).await?;
        }

        for (txid, utxos) in stasis.into_iter() {
            let record = TransactionRecord::new_stasis(self, txid, &utxos);
            self.processor().notify(Events::Stasis { record }).await?;
        }

        Ok(())
    }

    pub async fn register_addresses(&self, addresses: &[Address]) -> Result<()> {
        if addresses.is_empty() {
            log_error!("utxo processor: register for an empty address set");
        }

        let local = self.addresses();

        // addresses are filtered for a known address set where
        // addresses can already be registered with the processor
        // as a part of address space (Scan window) pre-caching.
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

        if addresses.is_not_empty() {
            self.processor().register_addresses(addresses, self).await?;
        }

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
            log_warn!("utxo processor: unregister for an empty address set")
        }

        Ok(())
    }

    pub async fn scan_and_register_addresses(&self, addresses: Vec<Address>, current_daa_score: Option<u64>) -> Result<()> {
        self.register_addresses(&addresses).await?;
        let resp = self.processor().rpc_api().get_utxos_by_addresses(addresses).await?;
        let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();
        let current_daa_score = current_daa_score.or_else(|| {
                self.processor()
                    .current_daa_score()
            }).ok_or(Error::MissingDaaScore("Expecting DAA score or initialized UtxoProcessor when invoking scan_and_register_addresses() - You might be accessing UtxoProcessor APIs before it is initialized (see `utxo-proc-start` event)"))?;
        self.extend_from_scan(refs, current_daa_score).await?;
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
