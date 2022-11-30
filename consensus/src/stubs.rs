use std::sync::Arc;

use crate::consensus::test_consensus::TestConsensus;
use consensus_core::{
    api::ConsensusApi,
    block::MutableBlock,
    blockhash::new_unique,
    header::Header,
    stubs::{BlockTemplate, MinerData},
    tx::Transaction,
};

/// A dummy ConsensusApi implementation while waiting for michealsutton's simpa PR being merged
impl ConsensusApi for TestConsensus {
    fn build_block_template(self: Arc<Self>, miner_data: MinerData, txs: Vec<Transaction>) -> BlockTemplate {
        let header = Header::new(0, vec![], new_unique(), 0, 0, 0, 0, 0.into(), 0);
        let block = MutableBlock::new(header, txs);
        BlockTemplate::new(block, miner_data, false, 0)
    }
}
