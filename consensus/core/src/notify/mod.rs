use std::collections::HashSet;

use muhash::MuHash;

use crate::{
    block::Block, coinbase::BlockRewardData, tx::TransactionId, utxo::utxo_diff::UtxoDiff, BlockHashMap,
    BlockHashSet,
};

use hashes::Hash;

#[derive(Debug, Clone)]
pub enum ConsensusNotification {
    BlockAdded(BlockAddedNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
    VirtualStateChange(VirtualStateChangeSetNotification),
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
