use crate::{block::Block};

#[derive(Debug, Clone)]
pub enum ConsensusNotification {
    BlockAdded(BlockAddedNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
    VirtualStateChangeSet(VirtualStateChangeSetNotification),
    PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification),
}

#[derive(Debug, Clone)]
pub struct VirtualStateChangeSetNotification {
    //TODO
}

#[derive(Debug, Clone)]
pub struct BlockAddedNotification {
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct NewBlockTemplateNotification {}

#[derive(Debug, Clone)]
pub struct PruningPointUTXOSetOverrideNotification {}
