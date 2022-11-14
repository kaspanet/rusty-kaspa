use consensus::{
    consensus::{test_consensus::create_temp_db, Consensus},
    model::stores::{
        block_transactions::BlockTransactionsStoreReader, headers::HeaderStoreReader, relations::RelationsStoreReader,
        statuses::BlockStatus,
    },
    params::{Params, DEVNET_PARAMS},
};
use consensus_core::{block::Block, BlockHashSet, HashMapCustomHasher};
use hashes::Hash;
use simulator::network::KaspaNetworkSimulator;
use std::{collections::VecDeque, sync::Arc};

pub mod simulator;

fn main() {
    let bps = 8.0;
    let delay = 2.0;
    let num_miners = 8;
    let until = 500 * 1000; // In milliseconds
    let params = DEVNET_PARAMS.clone_with_skip_pow();
    let mut sim = KaspaNetworkSimulator::new(delay, bps, &params);
    let (consensus, handles, _lifetime) = sim.init(num_miners).run(until);
    consensus.shutdown(handles);
    let (_lifetime2, db2) = create_temp_db();
    let consensus2 = Arc::new(Consensus::new(db2, &params));
    let handles2 = consensus2.init();
    let start = std::time::Instant::now();
    validate(&consensus, &consensus2, &params);
    eprintln!("Validation elapsed {:?}", start.elapsed());
    consensus2.shutdown(handles2);
    drop(consensus);
}

#[tokio::main]
async fn validate(consensus: &Arc<Consensus>, consensus2: &Arc<Consensus>, params: &Params) {
    let hashes = topologically_ordered_hashes(consensus, params);
    eprintln!("Validating {} blocks", hashes.len());
    for (i, &hash) in hashes.iter().enumerate() {
        let block =
            Block::from_arcs(consensus.headers_store.get_header(hash).unwrap(), consensus.block_transactions_store.get(hash).unwrap());
        let status =
            consensus2.validate_and_insert_block(block).await.unwrap_or_else(|e| panic!("block {} {} failed: {}", i, hash, e));
        assert!(status.is_utxo_valid_or_pending());
    }
    // Assert that at least one body tip was resolved with valid UTXO
    assert!(consensus2.body_tips().iter().copied().any(|h| consensus2.block_status(h) == BlockStatus::StatusUTXOValid));
}

fn topologically_ordered_hashes(consensus: &Arc<Consensus>, params: &Params) -> Vec<Hash> {
    let mut queue: VecDeque<Hash> = std::iter::once(params.genesis_hash).collect();
    let mut visited = BlockHashSet::new();
    let mut vec = Vec::new();
    let relations = consensus.relations_store.read();
    while let Some(current) = queue.pop_front() {
        for child in relations.get_children(current).unwrap().iter() {
            if visited.insert(*child) {
                queue.push_back(*child);
                vec.push(*child);
            }
        }
    }
    vec.sort_by_cached_key(|&h| consensus.headers_store.get_timestamp(h).unwrap());
    vec
}
