use std::sync::Arc;

use consensus_core::{block::Block, tx::TransactionId};
use hashes::Hash;
use utxoindex::model::UtxoSetByScriptPublicKey;

#[derive(Debug, Clone)]
pub enum Notification {
    BlockAdded(Arc<BlockAddedNotification>),
    NewBlockTemplate(NewBlockTemplateNotification),
    UtxosChanged(Arc<UtxosChangedNotification>),
    SinkBlueScoreChanged(SinkBlueScoreChangedNotification),
    VirtualChainChanged(Arc<VirtualChainChangedNotification>),
    VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification),
    PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification),
    FinalityConflict(FinalityConflictNotification),
    FinalityConflictResolved(FinalityConflictResolvedNotification),
}

#[derive(Debug, Clone)]
pub struct VirtualChainChangedNotification {
    pub added_chain_block_hashes: Arc<Vec<Hash>>,
    pub removed_chain_block_hashes: Arc<Vec<Hash>>,
    pub accepted_transaction_ids: Arc<Vec<TransactionId>>,
}

#[derive(Debug, Clone)]
pub struct VirtualDaaScoreChangedNotification {
    pub virtual_daa_score: u64,
}

#[derive(Debug, Clone)]
pub struct SinkBlueScoreChangedNotification {
    pub sink_blue_score: u64,
}

#[derive(Debug, Clone)]
pub struct BlockAddedNotification {
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct NewBlockTemplateNotification {}

#[derive(Debug, Clone, Default)]
pub struct PruningPointUTXOSetOverrideNotification {}

#[derive(Debug, Clone, Default)]
pub struct FinalityConflictNotification {}

#[derive(Debug, Clone, Default)]
pub struct FinalityConflictResolvedNotification {}

#[derive(Debug, Clone)]
pub struct UtxosChangedNotification {
    pub added: Arc<UtxoSetByScriptPublicKey>,
    pub removed: Arc<UtxoSetByScriptPublicKey>,
}
