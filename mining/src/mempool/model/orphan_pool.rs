use crate::mempool::{
    config::Config,
    errors::{RuleError, RuleResult},
    model::{
        map::{IdToTransactionMap, OutpointToIdMap},
        pool::Pool,
        tx::{MempoolTransaction, OrphanTransaction},
    },
};
use consensus_core::{
    api::DynConsensus,
    tx::MutableTransaction,
    tx::{TransactionId, TransactionOutpoint},
};
use kaspa_core::warn;
use std::{collections::HashMap, rc::Rc};

type IdToOrphanMap = HashMap<TransactionId, OrphanTransaction>;

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
    consensus: DynConsensus,
    config: Rc<Config>,
    all_orphans: IdToOrphanMap,
    orphan_ids_by_previous_outpoint: OutpointToIdMap,
    last_expire_scan: u64,
}

impl OrphanPool {
    pub(crate) fn new(consensus: DynConsensus, config: Rc<Config>) -> Self {
        Self {
            consensus,
            config,
            all_orphans: IdToOrphanMap::new(),
            orphan_ids_by_previous_outpoint: OutpointToIdMap::new(),
            last_expire_scan: 0,
        }
    }

    pub(crate) fn consensus(&self) -> DynConsensus {
        self.consensus.clone()
    }

    pub(crate) fn outpoint_orphan(&self, outpoint: &TransactionOutpoint) -> Option<&OrphanTransaction> {
        self.orphan_ids_by_previous_outpoint.get(outpoint).and_then(|id| self.all_orphans.get(id))
    }

    pub(crate) fn outpoint_orphan_mut(&mut self, outpoint: &TransactionOutpoint) -> Option<&mut OrphanTransaction> {
        self.orphan_ids_by_previous_outpoint.get(outpoint).and_then(|id| self.all_orphans.get_mut(id))
    }

    pub(crate) fn get_redeemer_ids(&self, transaction_id: &TransactionId) -> Vec<TransactionId> {
        let mut redeemers = Vec::new();
        if let Some(transaction) = self.all_orphans.get(transaction_id) {
            let mut stack = vec![transaction];
            while !stack.is_empty() {
                let transaction = stack.pop().unwrap();
                let mut outpoint = TransactionOutpoint { transaction_id: transaction.id(), index: 0 };
                for i in 0..transaction.mtx.tx.outputs.len() {
                    outpoint.index = i as u32;
                    if let Some(orphan) = self.outpoint_orphan(&outpoint) {
                        stack.push(orphan);
                        redeemers.push(orphan.id());
                    }
                }
            }
        }
        redeemers
    }

    pub(crate) fn maybe_add_orphan(&mut self, transaction: MutableTransaction, is_high_priority: bool) -> RuleResult<()> {
        if self.config.maximum_orphan_transaction_count == 0 {
            // TODO: determine how/why this may happen
            return Ok(());
        }
        self.check_orphan_duplicate(&transaction)?;
        self.check_orphan_mass(&transaction)?;
        self.check_orphan_double_spend(&transaction)?;
        self.add_orphan(transaction, is_high_priority)?;
        self.limit_orphan_pool_size()?;
        Ok(())
    }

    fn limit_orphan_pool_size(&mut self) -> RuleResult<()> {
        while self.all_orphans.len() as u64 > self.config.maximum_orphan_transaction_count {
            let orphan_to_remove = self.get_random_non_high_priority_orphan();
            if orphan_to_remove.is_none() {
                // this means all orphans are high priority
                warn!(
                    "Number of high-priority transactions in orphanPool ({0}) is higher than maximum allowed ({1})",
                    self.all_orphans.len(),
                    self.config.maximum_orphan_transaction_count
                );
                break;
            }
            // Don't remove redeemers in the case of a random eviction since the evicted transaction is
            // not invalid, therefore it's redeemers are as good as any orphan that just arrived.
            self.remove_orphan(&orphan_to_remove.unwrap().id(), false)?;
        }
        Ok(())
    }

    fn check_orphan_mass(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        if transaction.calculated_mass.unwrap() > self.config.maximum_orphan_transaction_mass {
            return Err(RuleError::RejectBadOrphanMass(
                transaction.calculated_mass.unwrap(),
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
                return Err(RuleError::RejectDoubleSpendOrphan(transaction.id(), double_spend_orphan.id()));
            }
        }
        Ok(())
    }

    fn add_orphan(&mut self, transaction: MutableTransaction, is_high_priority: bool) -> RuleResult<()> {
        let transaction = MempoolTransaction::new(transaction, is_high_priority, self.consensus.clone().get_virtual_daa_score());
        for input in transaction.mtx.tx.inputs.iter() {
            self.orphan_ids_by_previous_outpoint.insert(input.previous_outpoint, transaction.id());
        }
        self.all_orphans.insert(transaction.id(), transaction);
        Ok(())
    }

    pub(crate) fn remove_orphan(
        &mut self,
        transaction_id: &TransactionId,
        remove_redeemers: bool,
    ) -> RuleResult<Vec<MempoolTransaction>> {
        let mut transaction_ids_to_remove = vec![*transaction_id];
        if remove_redeemers {
            transaction_ids_to_remove.append(&mut self.get_redeemer_ids(transaction_id));
        }
        transaction_ids_to_remove.iter().map(|x| self.remove_single_orphan(x)).collect()
    }

    fn remove_single_orphan(&mut self, transaction_id: &TransactionId) -> RuleResult<MempoolTransaction> {
        if let Some(transaction) = self.all_orphans.remove(transaction_id) {
            for (i, input) in transaction.mtx.tx.inputs.iter().enumerate() {
                if self.orphan_ids_by_previous_outpoint.remove(&input.previous_outpoint).is_none() {
                    return Err(RuleError::RejectMissingOrphanOutpoint(i, transaction.id(), input.previous_outpoint));
                }
            }
            Ok(transaction)
        } else {
            Err(RuleError::RejectMissingOrphanTransaction(*transaction_id))
        }
    }

    pub(crate) fn remove_redeemers_of(&mut self, transaction_id: &TransactionId) -> RuleResult<Vec<MempoolTransaction>> {
        self.get_redeemer_ids(transaction_id).iter().map(|x| self.remove_single_orphan(x)).collect()
    }

    pub(crate) fn expire_orphan_transactions(&mut self) -> RuleResult<()> {
        let virtual_daa_score = self.consensus().get_virtual_daa_score();
        if virtual_daa_score - self.last_expire_scan < self.config.orphan_expire_scan_interval_daa_score {
            return Ok(());
        }

        // Never expire high priority transactions
        // Remove all transactions whose addedAtDAAScore is older then TransactionExpireIntervalDAAScore
        let expired_low_priority_transactions: Vec<TransactionId> = self
            .all_orphans
            .values()
            .filter(|x| !x.is_high_priority && virtual_daa_score - x.added_at_daa_score > self.config.orphan_expire_interval_daa_score)
            .map(|x| x.id())
            .collect();

        for transaction_id in expired_low_priority_transactions.iter() {
            self.remove_orphan(transaction_id, false)?;
        }

        self.last_expire_scan = virtual_daa_score;
        Ok(())
    }

    pub(crate) fn update_orphans_after_transaction_removed(
        &mut self,
        removed_transaction: &MempoolTransaction,
        remove_redeemers: bool,
    ) -> RuleResult<()> {
        let removed_transaction_id = removed_transaction.id();
        if remove_redeemers {
            self.remove_redeemers_of(&removed_transaction_id)?;
            return Ok(());
        }

        let mut outpoint = TransactionOutpoint { transaction_id: removed_transaction_id, index: 0 };
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
        Ok(())
    }

    fn get_random_non_high_priority_orphan(&self) -> Option<&OrphanTransaction> {
        self.all_orphans.values().find(|x| !x.is_high_priority)
    }

    // pub(crate) fn get_orphan_transactions_by_addresses(&self) -> RuleResult<IOScriptToTransaction> {
    //     todo!()
    // }
}

impl Pool for OrphanPool {
    fn all(&self) -> &IdToTransactionMap {
        &self.all_orphans
    }

    fn all_mut(&mut self) -> &mut IdToTransactionMap {
        &mut self.all_orphans
    }
}
