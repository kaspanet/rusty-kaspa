use std::{sync::Arc, thread::JoinHandle};

use consensus_core::{block::Block, header::Header};
use hashes::Hash;
use parking_lot::RwLock;

use crate::{
    model::stores::{ghostdag::DbGhostdagStore, reachability::DbReachabilityStore, DB},
    params::Params,
    pipeline::header_processor::HeaderProcessingContext,
    test_helpers::header_from_precomputed_hash,
};

use super::consensus::Consensus;

pub struct TestConsensus {
    consensus: Consensus,
    params: Params,
}

impl TestConsensus {
    pub fn new(db: Arc<DB>, params: &Params) -> Self {
        Self { consensus: Consensus::new(db, params), params: params.clone() }
    }

    pub fn build_header_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> Header {
        let mut header = header_from_precomputed_hash(hash, parents);
        let mut ctx: HeaderProcessingContext = HeaderProcessingContext::new(hash, &header);
        self.consensus
            .ghostdag_manager
            .add_block(&mut ctx, hash);

        let ghostdag_data = ctx.ghostdag_data.unwrap();
        let window = self
            .consensus
            .dag_traversal_manager
            .block_window(ghostdag_data.clone(), self.params.difficulty_window_size);

        let window_hashes = window
            .into_iter()
            .map(|item| item.0.hash)
            .collect();

        let (daa_score, _) = self
            .consensus
            .difficulty_manager
            .calc_daa_score_and_added_blocks(&window_hashes, &ghostdag_data);

        header.daa_score = daa_score;

        header.timestamp = self
            .consensus
            .past_median_time_manager
            .calc_past_median_time(ghostdag_data)
            + 1;
        header
    }

    pub fn build_block_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> Block {
        Block::from_header(self.build_header_with_parents(hash, parents))
    }

    pub fn validate_and_insert_block(&self, block: Arc<Block>) {
        self.consensus.validate_and_insert_block(block)
    }

    pub fn init(&self) -> JoinHandle<()> {
        self.consensus.init()
    }

    pub fn drop(self) -> (Arc<RwLock<DbReachabilityStore>>, Arc<DbGhostdagStore>) {
        self.consensus.drop()
    }
}
