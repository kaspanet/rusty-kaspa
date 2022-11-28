use crate::{block::MutableBlock, tx::ScriptPublicKey};

// TODO: Remove all these items when michealsutton's simpa branch has been merged.

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct MinerData<T: AsRef<[u8]> = Vec<u8>> {
    pub script_public_key: ScriptPublicKey,
    pub extra_data: T,
}

impl<T: AsRef<[u8]>> MinerData<T> {
    pub fn new(script_public_key: ScriptPublicKey, extra_data: T) -> Self {
        Self { script_public_key, extra_data }
    }
}

/// A block template for miners.
#[derive(Debug, Clone)]
pub struct BlockTemplate {
    pub block: MutableBlock,
    pub miner_data: MinerData,

    /// Should the miner be directly rewarded for merging red blocks?
    pub coinbase_has_red_reward: bool,

    /// Replaces golang kaspad DomainBlockTemplate.IsNearlySynced
    pub selected_parent_timestamp: u64,
}

impl BlockTemplate {
    pub fn new(block: MutableBlock, miner_data: MinerData, coinbase_has_red_reward: bool, selected_parent_timestamp: u64) -> Self {
        Self { block, miner_data, coinbase_has_red_reward, selected_parent_timestamp }
    }
}
