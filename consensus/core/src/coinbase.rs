use crate::tx::{ScriptPublicKey, Transaction};

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct MinerData<'a> {
    pub script_public_key: ScriptPublicKey,
    pub extra_data: &'a [u8],
}

#[derive(PartialEq, Eq, Debug)]
pub struct CoinbaseData<'a> {
    pub blue_score: u64,
    pub subsidy: u64,
    pub miner_data: MinerData<'a>,
}

pub struct BlockRewardData {
    pub subsidy: u64,
    pub total_fees: u64,
    pub script_public_key: ScriptPublicKey,
}

impl BlockRewardData {
    pub fn new(subsidy: u64, total_fees: u64, script_public_key: ScriptPublicKey) -> Self {
        Self { subsidy, total_fees, script_public_key }
    }
}

/// Holds a coinbase transaction along with meta-data obtained during creation
pub struct CoinbaseTransactionTemplate {
    pub tx: Transaction,
    pub has_red_reward: bool, // Does the last output contain reward for red blocks
}
