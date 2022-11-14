use clap::Parser;
use consensus::{
    consensus::{test_consensus::create_temp_db, Consensus},
    errors::{BlockProcessResult, RuleError},
    model::stores::{
        block_transactions::BlockTransactionsStoreReader, ghostdag::GhostdagStoreReader, headers::HeaderStoreReader,
        relations::RelationsStoreReader, statuses::BlockStatus,
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
    #[arg(short, long, default_value_t = 2)]
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

/// Calculates the k parameter of the GHOSTDAG protocol such that anticones lager than k will be created
/// with probability less than 'delta' (follows eq. 1 from section 4.2 of the PHANTOM paper)
/// `x` is expected to be 2Dλ where D is the maximal network delay and λ is the block mining rate.
/// `delta` is an upper bound for the probability of anticones larger than k.
/// Returns the minimal k such that the above conditions hold.
fn calculate_ghostdag_k(x: f64, delta: f64) -> u64 {
    assert!(x > 0.0);
    assert!(delta > 0.0 && delta < 1.0);
    let (mut k_hat, mut sigma, mut fraction, exp) = (0u64, 0.0, 1.0, std::f64::consts::E.powf(-x));
    loop {
        sigma += exp * fraction;
        if 1.0 - sigma < delta {
            return k_hat;
        }
        k_hat += 1;
        fraction *= x / k_hat as f64 // Computes x^k_hat/k_hat!
    }
}

fn main() {
    let args = Args::parse();
    let mut params = DEVNET_PARAMS.clone_with_skip_pow();
    if args.bps > 1.0 || args.delay > 2.0 {
        let k = u64::max(calculate_ghostdag_k(2.0 * args.delay * args.bps, 0.05), params.ghostdag_k as u64);
        let k = u64::min(k, u8::MAX as u64) as u8; // Clamp to u8::MAX
        params.ghostdag_k = k;
        params.mergeset_size_limit = k as u64 * 10;
        params.max_block_parents = u8::max((0.66 * k as f64) as u8, 10);
        params.target_time_per_block = (1000.0 / args.bps) as u64;
        params.merge_depth = (params.merge_depth as f64 * args.bps) as u64;
        params.difficulty_window_size = (params.difficulty_window_size as f64 * args.bps) as usize; // Scale the DAA window linearly with BPS
        println!("The delay times bps product is larger than 2 (2Dλ={}), setting GHOSTDAG K={}", 2.0 * args.delay * args.bps, k);
    }
    let until = args.sim_time * 1000; // milliseconds
    let mut sim = KaspaNetworkSimulator::new(args.delay, args.bps, &params);
    let (consensus, handles, _lifetime) = sim.init(args.miners, args.tpb, !args.quiet).run(until);
    consensus.shutdown(handles);
    let (_lifetime2, db2) = create_temp_db();
    let consensus2 = Arc::new(Consensus::new(db2, &params));
    let handles2 = consensus2.init();
    validate(&consensus, &consensus2, &params, args.delay, args.bps);
    consensus2.shutdown(handles2);
    drop(consensus);
}

#[tokio::main]
async fn validate(src_consensus: &Consensus, dst_consensus: &Consensus, params: &Params, delay: f64, bps: f64) {
    let hashes = topologically_ordered_hashes(src_consensus, params.genesis_hash);
    print_stats(src_consensus, &hashes, delay, bps, params.ghostdag_k);
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

fn topologically_ordered_hashes(src_consensus: &Consensus, genesis_hash: Hash) -> Vec<Hash> {
    let mut queue: VecDeque<Hash> = std::iter::once(genesis_hash).collect();
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

fn print_stats(src_consensus: &Consensus, hashes: &[Hash], delay: f64, bps: f64, k: u8) {
    let blues_mean = hashes.iter().map(|&h| src_consensus.ghostdag_store.get_data(h).unwrap().mergeset_blues.len()).sum::<usize>()
        as f64
        / hashes.len() as f64;
    let reds_mean = hashes.iter().map(|&h| src_consensus.ghostdag_store.get_data(h).unwrap().mergeset_reds.len()).sum::<usize>()
        as f64
        / hashes.len() as f64;
    let parents_mean = hashes.iter().map(|&h| src_consensus.headers_store.get_header(h).unwrap().direct_parents().len()).sum::<usize>()
        as f64
        / hashes.len() as f64;
    let txs_mean = hashes.iter().map(|&h| src_consensus.block_transactions_store.get(h).unwrap().len()).sum::<usize>() as f64
        / hashes.len() as f64;
    println!("[DELAY={}, BPS={}, GHOSTDAG K={}]", delay, bps, k);
    println!(
        "[Average stats of generated DAG] blues: {}, reds: {}, parents: {}, txs: {}",
        blues_mean, reds_mean, parents_mean, txs_mean
    );
}
