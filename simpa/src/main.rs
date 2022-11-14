use clap::Parser;
use consensus::{
    consensus::{test_consensus::create_temp_db, Consensus},
    errors::{BlockProcessResult, RuleError},
    model::stores::{
        block_transactions::BlockTransactionsStoreReader, headers::HeaderStoreReader, relations::RelationsStoreReader,
        statuses::BlockStatus,
    },
    params::{Params, DEVNET_PARAMS},
};
use consensus_core::{block::Block, BlockHashSet, HashMapCustomHasher};
use futures::{future::join_all, Future};
use hashes::Hash;
use itertools::Itertools;
use simulator::network::KaspaNetworkSimulator;
use std::{collections::VecDeque, sync::Arc};

pub mod simulator;

/// Kaspa Network Simulator
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Simulation blocks per second
    #[arg(short, long, default_value_t = 8.0)]
    bps: f64,

    /// Simulation delay (seconds)
    #[arg(short, long, default_value_t = 2.0)]
    delay: f64,

    /// Number of miners
    #[arg(short, long, default_value_t = 8)]
    miners: u64,

    /// Target transactions per block
    #[arg(short, long, default_value_t = 100)]
    tpb: u64,

    /// Target simulation time (seconds)
    #[arg(short, long, default_value_t = 50)]
    sim_time: u64,

    /// Avoid verbose simulation information
    #[arg(short, long, default_value_t = false)]
    quiet: bool,
}

fn main() {
    let args = Args::parse();
    let until = args.sim_time * 1000; // milliseconds
    let params = DEVNET_PARAMS.clone_with_skip_pow();
    let mut sim = KaspaNetworkSimulator::new(args.delay, args.bps, &params);
    let (consensus, handles, _lifetime) = sim.init(args.miners, args.tpb, !args.quiet).run(until);
    consensus.shutdown(handles);
    let (_lifetime2, db2) = create_temp_db();
    let consensus2 = Arc::new(Consensus::new(db2, &params));
    let handles2 = consensus2.init();
    validate(&consensus, &consensus2, &params);
    consensus2.shutdown(handles2);
    drop(consensus);
}

#[tokio::main]
async fn validate(src_consensus: &Consensus, dst_consensus: &Consensus, params: &Params) {
    let hashes = topologically_ordered_hashes(src_consensus, params);
    eprintln!("Validating {} blocks", hashes.len());
    let start = std::time::Instant::now();
    let chunks = hashes.into_iter().chunks(1000);
    let mut iter = chunks.into_iter();
    let mut chunk = iter.next().unwrap();
    let mut prev_joins = submit_chunk(src_consensus, dst_consensus, &mut chunk);

    for mut chunk in iter {
        let current_joins = submit_chunk(src_consensus, dst_consensus, &mut chunk);
        let statuses = join_all(prev_joins).await.into_iter().collect::<Result<Vec<BlockStatus>, RuleError>>().unwrap();
        assert!(statuses.iter().all(|s| s.is_utxo_valid_or_pending()));
        prev_joins = current_joins;
    }

    let statuses = join_all(prev_joins).await.into_iter().collect::<Result<Vec<BlockStatus>, RuleError>>().unwrap();
    assert!(statuses.iter().all(|s| s.is_utxo_valid_or_pending()));

    // Assert that at least one body tip was resolved with valid UTXO
    assert!(dst_consensus.body_tips().iter().copied().any(|h| dst_consensus.block_status(h) == BlockStatus::StatusUTXOValid));
    eprintln!("Validation time: {:?}", start.elapsed());
}

fn submit_chunk(
    src_consensus: &Consensus,
    dst_consensus: &Consensus,
    chunk: &mut impl Iterator<Item = Hash>,
) -> Vec<impl Future<Output = BlockProcessResult<BlockStatus>>> {
    let mut futures = Vec::new();
    for hash in chunk {
        let block = Block::from_arcs(
            src_consensus.headers_store.get_header(hash).unwrap(),
            src_consensus.block_transactions_store.get(hash).unwrap(),
        );
        let f = dst_consensus.validate_and_insert_block(block);
        futures.push(f);
    }
    futures
}

fn topologically_ordered_hashes(src_consensus: &Consensus, params: &Params) -> Vec<Hash> {
    let mut queue: VecDeque<Hash> = std::iter::once(params.genesis_hash).collect();
    let mut visited = BlockHashSet::new();
    let mut vec = Vec::new();
    let relations = src_consensus.relations_store.read();
    while let Some(current) = queue.pop_front() {
        for child in relations.get_children(current).unwrap().iter() {
            if visited.insert(*child) {
                queue.push_back(*child);
                vec.push(*child);
            }
        }
    }
    vec.sort_by_cached_key(|&h| src_consensus.headers_store.get_timestamp(h).unwrap());
    vec
}
