use crate::{block::Block, utxo::utxo_diff::UtxoDiff};
use hashes::Hash;

#[derive(Debug, Clone)]
pub enum ConsensusNotification {
    BlockAdded(BlockAddedNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
    VirtualChangeSet(VirtualChangeSetNotification),
    PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification),
}

#[derive(Debug, Clone)]
pub struct VirtualChangeSetNotification {
    pub virtual_utxo_diff: UtxoDiff,
    pub virtual_parents: Vec<Hash>,
    pub virtual_selected_parent_blue_score: u64,
    pub virtual_daa_score: u64,
}

#[derive(Debug, Clone)]
pub struct BlockAddedNotification {
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct NewBlockTemplateNotification {}

#[derive(Debug, Clone)]
pub struct PruningPointUTXOSetOverrideNotification {}
