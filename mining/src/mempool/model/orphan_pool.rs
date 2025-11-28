use crate::mempool::{
    config::Config,
    errors::{RuleError, RuleResult},
    model::{
        map::{MempoolTransactionCollection, OutpointIndex},
        pool::{Pool, TransactionsEdges},
        tx::{MempoolTransaction, TxRemovalReason},
    },
    tx::Priority,
};
use kaspa_consensus_core::{
    tx::MutableTransaction,
    tx::{TransactionId, TransactionOutpoint},
};
use kaspa_core::{debug, warn};
use kaspa_utils::iter::IterExtensions;
use std::sync::Arc;

/// Pool of orphan transactions depending on some missing utxo entries
///
/// ### Rust rewrite notes
///
/// The 2 main design decisions are that [TransactionPool] and [OrphanPool] are
/// both storing [MempoolTransaction]s instead of having distinct structures
/// and these object are owned respectively by all_transactions and all_orphans
/// fields without any other external reference so no smart pointer is needed.
///
/// This has following consequences:
///
/// - orphansByPreviousOutpoint maps an id instead of a transaction reference
///   introducing a indirection stage when the matching object is required.
/// - "un-orphaning" a transaction induces a move of the object from orphan
///   to transactions pool with no reconstruction nor cloning.
pub(crate) struct OrphanPool {
    config: Arc<Config>,
    all_orphans: MempoolTransactionCollection,
    /// Transactions dependencies formed by outputs present in pool - successor relations.
    chained_orphans: TransactionsEdges,
    outpoint_owner_id: OutpointIndex,
    last_expire_scan: u64,
}

impl OrphanPool {
    pub(crate) fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            all_orphans: MempoolTransactionCollection::default(),
            chained_orphans: TransactionsEdges::default(),
            outpoint_owner_id: OutpointIndex::default(),
            last_expire_scan: 0,
        }
    }

    pub(crate) fn outpoint_orphan(&self, outpoint: &TransactionOutpoint) -> Option<&MempoolTransaction> {
        self.outpoint_owner_id.get(outpoint).and_then(|id| self.all_orphans.get(id))
    }

    pub(crate) fn outpoint_orphan_mut(&mut self, outpoint: &TransactionOutpoint) -> Option<&mut MempoolTransaction> {
        self.outpoint_owner_id.get(outpoint).and_then(|id| self.all_orphans.get_mut(id))
    }

    pub(crate) fn try_add_orphan(
        &mut self,
        virtual_daa_score: u64,
        transaction: MutableTransaction,
        priority: Priority,
    ) -> RuleResult<()> {
        // Rust rewrite: original name is maybeAddOrphan
        if self.config.maximum_orphan_transaction_count == 0 {
            // TODO: determine how/why this may happen
            return Ok(());
        }
        self.check_orphan_duplicate(&transaction)?;
        self.check_orphan_mass(&transaction)?;
        self.check_orphan_double_spend(&transaction)?;
        // Make sure there is room in the pool for the new transaction
        self.limit_orphan_pool_size(1)?;
        self.add_orphan(virtual_daa_score, transaction, priority)?;
        Ok(())
    }

    /// Make room in the pool for at least `free_slots` new transactions.
    ///
    /// An error is returned if the pool is filled with high priority transactions.
    fn limit_orphan_pool_size(&mut self, free_slots: usize) -> RuleResult<()> {
        while self.all_orphans.len() + free_slots > self.config.maximum_orphan_transaction_count as usize {
            let orphan_to_remove = self.get_random_low_priority_orphan();
            if orphan_to_remove.is_none() {
                // this means all orphans are high priority so return an error
                let err = RuleError::RejectOrphanPoolIsFull(self.all_orphans.len(), self.config.maximum_orphan_transaction_count);
                warn!("{}", err.to_string());
                return Err(err);
            }
            // Don't remove redeemers in the case of a random eviction since the evicted transaction is
            // not invalid, therefore it's redeemers are as good as any orphan that just arrived.
            self.remove_orphan(&orphan_to_remove.unwrap().id(), false, TxRemovalReason::MakingRoom, "")?;
        }
        Ok(())
    }

    fn check_orphan_mass(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        if transaction.calculated_non_contextual_masses.unwrap().max() > self.config.maximum_orphan_transaction_mass {
            return Err(RuleError::RejectBadOrphanMass(
                transaction.calculated_non_contextual_masses.unwrap().max(),
                self.config.maximum_orphan_transaction_mass,
            ));
        }
        Ok(())
    }

    fn check_orphan_duplicate(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        if self.all_orphans.contains_key(&transaction.id()) {
            return Err(RuleError::RejectDuplicateOrphan(transaction.id()));
        }
        Ok(())
    }

    fn check_orphan_double_spend(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        for input in transaction.tx.inputs.iter() {
            if let Some(double_spend_orphan) = self.outpoint_orphan(&input.previous_outpoint) {
                if double_spend_orphan.id() != transaction.id() {
                    return Err(RuleError::RejectDoubleSpendOrphan(transaction.id(), double_spend_orphan.id()));
                }
            }
        }
        Ok(())
    }

    fn add_orphan(&mut self, virtual_daa_score: u64, transaction: MutableTransaction, priority: Priority) -> RuleResult<()> {
        let id = transaction.id();
        let transaction = MempoolTransaction::new(transaction, priority, virtual_daa_score);
        // Add all entries in outpoint_owner_id
        for input in transaction.mtx.tx.inputs.iter() {
            self.outpoint_owner_id.insert(input.previous_outpoint, id);
        }

        // Add all chained_transaction relations...
        // ... incoming
        for parent_id in self.get_parent_transaction_ids_in_pool(&transaction.mtx) {
            let entry = self.chained_mut().entry(parent_id).or_default();
            entry.insert(id);
        }
        // ... outgoing
        let mut outpoint = TransactionOutpoint::new(id, 0);
        for i in 0..transaction.mtx.tx.outputs.len() {
            outpoint.index = i as u32;
            if let Some(chained) = self.outpoint_orphan(&outpoint).map(|x| x.id()) {
                self.chained_mut().entry(id).or_default().insert(chained);
            }
        }

        self.all_orphans.insert(id, transaction);
        debug!("Added transaction to orphan pool: {}", id);
        Ok(())
    }

    pub(crate) fn remove_orphan(
        &mut self,
        transaction_id: &TransactionId,
        remove_redeemers: bool,
        reason: TxRemovalReason,
        extra_info: &str,
    ) -> RuleResult<Vec<MempoolTransaction>> {
        // Rust rewrite:
        // - the call cycle removeOrphan -> removeRedeemersOf -> removeOrphan is replaced by
        //   the sequence get_redeemer_ids_in_pool, remove_single_orphan
        // - recursion is removed (see get_redeemer_ids_in_pool)

        if !self.has(transaction_id) {
            return Ok(vec![]);
        }

        let mut transaction_ids_to_remove = vec![*transaction_id];
        if remove_redeemers {
            transaction_ids_to_remove.extend(self.get_redeemer_ids_in_pool(transaction_id));
        }
        let removed_transactions =
            transaction_ids_to_remove.iter().map(|x| self.remove_single_orphan(x)).collect::<RuleResult<Vec<_>>>()?;
        if reason.verbose() {
            match removed_transactions.len() {
                0 => (), // This is not possible
                1 => {
                    debug!("Removed orphan transaction ({}): {}{}", reason, removed_transactions[0].id(), extra_info);
                }
                n => {
                    debug!(
                        "Removed {} orphan transactions ({}): {}{}",
                        n,
                        reason,
                        removed_transactions.iter().map(|x| x.id()).reusable_format(", "),
                        extra_info
                    );
                }
            }
        }
        Ok(removed_transactions)
    }

    fn remove_single_orphan(&mut self, transaction_id: &TransactionId) -> RuleResult<MempoolTransaction> {
        if let Some(transaction) = self.all_orphans.remove(transaction_id) {
            // Remove all chained_transaction relations...
            // ... incoming
            let parents = self.get_parent_transaction_ids_in_pool(&transaction.mtx);
            parents.iter().for_each(|parent_id| {
                if let Some(entry) = self.chained_mut().get_mut(parent_id) {
                    entry.remove(transaction_id);
                    if entry.is_empty() {
                        self.chained_mut().remove(parent_id);
                    }
                }
            });
            // ... outgoing
            self.chained_mut().remove(transaction_id);

            // Remove all entries in outpoint_owner_id
            let mut error = None;
            for (i, input) in transaction.mtx.tx.inputs.iter().enumerate() {
                if self.outpoint_owner_id.remove(&input.previous_outpoint).is_none() {
                    error = Some(RuleError::RejectMissingOrphanOutpoint(i, transaction.id(), input.previous_outpoint));
                }
            }
            match error {
                None => Ok(transaction),
                Some(err) => Err(err),
            }
        } else {
            Err(RuleError::RejectMissingOrphanTransaction(*transaction_id))
        }
    }

    pub(crate) fn remove_redeemers_of(&mut self, transaction_id: &TransactionId) -> RuleResult<Vec<MempoolTransaction>> {
        self.get_redeemer_ids_in_pool(transaction_id).iter().map(|x| self.remove_single_orphan(x)).collect()
    }

    pub(crate) fn update_orphans_after_transaction_removed(
        &mut self,
        removed_transaction: &MempoolTransaction,
        remove_redeemers: bool,
    ) -> RuleResult<Vec<MempoolTransaction>> {
        let removed_transaction_id = removed_transaction.id();
        if remove_redeemers {
            return self.remove_redeemers_of(&removed_transaction_id);
        }

        let mut outpoint = TransactionOutpoint::new(removed_transaction_id, 0);
        for i in 0..removed_transaction.mtx.tx.outputs.len() {
            outpoint.index = i as u32;
            if let Some(orphan) = self.outpoint_orphan_mut(&outpoint) {
                for (i, input) in orphan.mtx.tx.inputs.iter().enumerate() {
                    if input.previous_outpoint.transaction_id == removed_transaction_id {
                        orphan.mtx.entries[i] = None;
                    }
                }
            }
        }
        Ok(vec![])
    }

    fn get_random_low_priority_orphan(&self) -> Option<&MempoolTransaction> {
        self.all_orphans.values().find(|x| x.priority == Priority::Low)
    }

    fn chained_mut(&mut self) -> &mut TransactionsEdges {
        &mut self.chained_orphans
    }

    pub(crate) fn expire_low_priority_transactions(&mut self, virtual_daa_score: u64) -> RuleResult<()> {
        if virtual_daa_score < self.last_expire_scan + self.config.orphan_expire_scan_interval_daa_score.after() {
            return Ok(());
        }

        // Never expire high priority transactions
        // Remove all transactions whose `added_at_daa_score` is older then TransactionExpireIntervalDAAScore
        let expired_low_priority_transactions: Vec<TransactionId> = self
            .all_orphans
            .values()
            .filter_map(|x| {
                if (x.priority == Priority::Low)
                    && virtual_daa_score > x.added_at_daa_score + self.config.orphan_expire_interval_daa_score.after()
                {
                    Some(x.id())
                } else {
                    None
                }
            })
            .collect();

        for transaction_id in expired_low_priority_transactions.iter() {
            self.remove_orphan(transaction_id, false, TxRemovalReason::Expired, "")?;
        }

        self.last_expire_scan = virtual_daa_score;
        Ok(())
    }
}

impl Pool for OrphanPool {
    fn all(&self) -> &MempoolTransactionCollection {
        &self.all_orphans
    }

    fn chained(&self) -> &TransactionsEdges {
        &self.chained_orphans
    }
}
