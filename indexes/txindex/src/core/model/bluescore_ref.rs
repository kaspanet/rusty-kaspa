use kaspa_consensus_core::tx::TransactionId;

use crate::stores::bluescore_refs::StoreIdent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueScoreRefData {
    pub blue_score: u64,
    pub txid: TransactionId,
    pub store_ident: StoreIdent,
}

pub enum BlueScoreRefQuery {
    Acceptance = 0,
    Inclusion,
    Both,
}
