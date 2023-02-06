use crate::{block::Block, utxo::utxo_diff::UtxoDiff};
use hashes::Hash;

//TODO: eventual clean-up, probably not all of these Notifications are actually emitted by consensus? some being from p2p, but for now placeholded here.
#[derive(Debug, Clone)]
pub enum ConsensusNotification {
    BlockAdded(BlockAddedNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
    VirtualChangeSet(VirtualChangeSetNotification),
    PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification),
    FinalityConflictResolved(FinalityConflictResolvedNotification),
    FinalityConflicts(FinalityConflictsNotification),
}

#[derive(Debug, Clone, Default)]
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

#[derive(Debug, Clone, Default)]
pub struct PruningPointUTXOSetOverrideNotification {}

impl PruningPointUTXOSetOverrideNotification {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug, Clone)]
pub struct FinalityConflictResolvedNotification {}

#[derive(Debug, Clone)]
pub struct FinalityConflictsNotification {}
