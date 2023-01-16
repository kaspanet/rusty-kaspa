use muhash::MuHash;

use crate::{
    block::Block, coinbase::BlockRewardData, tx::TransactionId, utxo::utxo_diff::UtxoDiff, BlockHashMap,
    BlockHashSet,
};
use hashes::Hash;

#[derive(Debug, Clone)]
pub enum Notification {
    BlockAdded(BlockAddedNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
    VirtualStateChange(VirtualStateChangeNotification),
    PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification),
}

#[derive(Debug, Clone)]
pub struct VirtualStateChangeNotification {
    pub parents: Vec<Hash>,
    //pub ghostdag_data: GhostdagData, //TODO: bring into scope somehow
    pub daa_score: u64,
    pub bits: u32,
    pub past_median_time: u64,
    pub multiset: MuHash,
    pub utxo_diff: UtxoDiff,
    pub accepted_tx_ids: Vec<TransactionId>, // TODO: consider saving `accepted_id_merkle_root` directly
    pub mergeset_rewards: BlockHashMap<BlockRewardData>,
    pub mergeset_non_daa: BlockHashSet,
}

#[derive(Debug, Clone)]
pub struct BlockAddedNotification {
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct NewBlockTemplateNotification {}

#[derive(Debug, Clone)]
pub struct PruningPointUTXOSetOverrideNotification {}
