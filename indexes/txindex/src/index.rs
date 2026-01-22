use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_notify::notification::{BlockAddedNotification, VirtualChainChangedNotification};

use crate::{
    errors::TxIndexResult,
    model::{
        bluescore_ref::{BlueScoreRefData, BlueScoreRefQuery},
        transactions::{TxAcceptanceData, TxInclusionData},
    },
};

pub struct TxIndex {}

impl Default for TxIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl TxIndex {
    pub fn new() -> Self {
        todo!()
    }

    pub fn ident(&self) -> &str {
        todo!()
    }

    pub fn get_accepted_transaction_data(&self, _txid: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>> {
        todo!()
    }

    pub fn get_included_transaction_data(&self, _txid: TransactionId) -> TxIndexResult<Vec<TxInclusionData>> {
        todo!()
    }

    pub fn scan_blue_scores(&self, _from: u64, _to: u64, _query: BlueScoreRefQuery) -> TxIndexResult<Option<Vec<BlueScoreRefData>>> {
        todo!()
    }

    pub fn update_via_block_added(&self, _block_added_notification: BlockAddedNotification) {
        todo!()
    }

    pub fn update_via_virtual_chain_changed(&self, _virtual_chain_changed_notification: VirtualChainChangedNotification) {
        todo!()
    }

    pub fn prune(&self, _range: u64) {
        todo!()
    }

    pub fn resync(&self) {
        todo!()
    }
}
