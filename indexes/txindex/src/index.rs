use std::{ops::RangeBounds, sync::Weak};

use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_notify::notification::{BlockAddedNotification, VirtualChainChangedNotification};
use kaspa_consensusmanager::ConsensusResetHandler;
use parking_lot::RwLock;

use crate::{
    errors::TxIndexResult,
    model::{
        bluescore_refs::BlueScoreRefData,
        transactions::{TxAcceptanceData, TxInclusionData},
    },
    reindexer::{block_added_reindexer, virtual_changed_reindexer},
    stores::store_manager::Store,
};

pub struct TxIndex {
    store: Store,
}

impl Default for TxIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl TxIndex {
    pub fn new() -> Self {
        todo!()
    }

    pub fn get_accepted_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>> {
        Ok(self.store.get_accepted_transaction_data(txid)?)
    }

    pub fn get_included_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxInclusionData>> {
        Ok(self.store.get_included_transaction_data(txid)?)
    }

    pub fn update_via_block_added(&mut self, block_added_notification: BlockAddedNotification) -> TxIndexResult<()> {
        let reindexed_block_added_state = block_added_reindexer::reindex_block_added_notification(&block_added_notification);
        Ok(self.store.update_with_new_block_added_state(reindexed_block_added_state)?)
    }

    pub fn update_via_virtual_chain_changed(
        &mut self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> TxIndexResult<()> {
        let reindexerd_virtual_changed_state =
            virtual_changed_reindexer::reindex_virtual_changed_notification(&virtual_chain_changed_notification);
        Ok(self.store.update_with_new_virtual_changed_state(reindexerd_virtual_changed_state)?)
    }

    /// Ranges are inclusive
    pub fn get_transaction_data_by_blue_score_range(
        &self,
        from: u64, // inclusive
        to: u64,   // inclusive
    ) -> TxIndexResult<Vec<BlueScoreRefData>> {
        Ok(self.store.get_transaction_data_by_blue_score_range(from..=to)?)
    }

    /// Ranges are inclusive
    pub fn get_transaction_inclusion_data_by_blue_score_range(
        &self,
        from: u64, // inclusive
        to: u64,   // inclusive
    ) -> TxIndexResult<Vec<BlueScoreRefData>> {
        Ok(self.store.get_transaction_inclusion_data_by_blue_score_range(from..=to)?)
    }
    /// Ranges are inclusive
    pub fn get_transaction_acceptance_data_by_blue_score_range(
        &self,
        from: u64, // inclusive
        to: u64,   // inclusive
    ) -> TxIndexResult<Vec<BlueScoreRefData>> {
        Ok(self.store.get_transaction_acceptance_data_by_blue_score_range(from..=to)?)
    }

    pub fn prune(&mut self) {
        todo!()
    }

    pub fn resync(&mut self) {
        todo!()
    }
}

impl std::fmt::Debug for TxIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxIndex").finish()
    }
}

struct TxIndexConsensusResetHandler {
    txindex: Weak<RwLock<TxIndex>>,
}

impl TxIndexConsensusResetHandler {
    fn new(txindex: Weak<RwLock<TxIndex>>) -> Self {
        Self { txindex }
    }
}

impl ConsensusResetHandler for TxIndexConsensusResetHandler {
    fn handle_consensus_reset(&self) {
        if let Some(txindex) = self.txindex.upgrade() {
            txindex.write().resync();
        }
    }
}
