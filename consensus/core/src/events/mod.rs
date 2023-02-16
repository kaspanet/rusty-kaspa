use crate::{block::Block, tx::TransactionId, utxo::utxo_diff::UtxoDiff};
use hashes::Hash;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum ConsensusEvent {
    BlockAdded(Arc<BlockAddedEvent>),
    NewBlockTemplate(Arc<NewBlockTemplateEvent>),
    VirtualChangeSet(Arc<VirtualChangeSetEvent>),
    PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideEvent),
    FinalityConflict(FinalityConflictEvent),
    FinalityConflictResolved(FinalityConflictResolvedEvent),
}

#[derive(Debug, Clone, Default)]
pub struct VirtualChangeSetEvent {
    // Note: This differs from the Consensus [`VirtualState`]'s [`UtxoDiff`] which  represents the utxo-state diff to the selected parent.
    // the utxo diff here represents the accumulated diff between the last virtual state and the current virtual state.
    pub accumulated_utxo_diff: Arc<UtxoDiff>,
    pub parents: Arc<Vec<Hash>>,
    pub selected_parent_blue_score: u64,
    pub daa_score: u64,
    pub mergeset_blues: Arc<Vec<Hash>>,
    pub mergeset_reds: Arc<Vec<Hash>>,
    pub accepted_tx_ids: Arc<Vec<TransactionId>>,
}
#[derive(Debug, Clone)]
pub struct BlockAddedEvent {
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct NewBlockTemplateEvent {}

#[derive(Debug, Clone, Default)]
pub struct PruningPointUTXOSetOverrideEvent {}

#[derive(Debug, Clone, Default)]
pub struct FinalityConflictEvent {}

#[derive(Debug, Clone, Default)]
pub struct FinalityConflictResolvedEvent {}
