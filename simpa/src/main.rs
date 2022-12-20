use clap::Parser;
use consensus::{
    consensus::{
        test_consensus::{create_temp_db, load_existing_db},
        Consensus,
    },
    constants::perf::{PerfParams, PERF_PARAMS},
    errors::{BlockProcessResult, RuleError},
    model::stores::{
        block_transactions::BlockTransactionsStoreReader,
        ghostdag::{GhostdagStoreReader, KType},
        headers::HeaderStoreReader,
        relations::RelationsStoreReader,
    },
    params::{Params, DEVNET_PARAMS},
    processes::ghostdag::ordering::SortableBlock,
};
use consensus_core::{block::Block, blockstatus::BlockStatus, header::Header, BlockHashSet, HashMapCustomHasher};
use futures::{future::join_all, Future};
use hashes::Hash;
use itertools::Itertools;
use simulator::network::KaspaNetworkSimulator;
use std::{collections::VecDeque, mem::size_of, sync::Arc};

pub mod simulator;

/// Kaspa Network Simulator
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Simulation blocks per second
    #[arg(short, long, default_value_t = 1.0)]
    bps: f64,

    /// Simulation delay (seconds)
    #[arg(short, long, default_value_t = 2.0)]
    delay: f64,

    /// Number of miners
    #[arg(short, long, default_value_t = 1)]
    miners: u64,

    /// Target transactions per block
    #[arg(short, long, default_value_t = 200)]
    tpb: u64,

    /// Target simulation time (seconds)
    #[arg(short, long, default_value_t = 600)]
    sim_time: u64,

    /// Target number of blocks the simulation should produce (overrides --sim-time if specified)
    #[arg(short = 'n', long)]
    target_blocks: Option<u64>,

    /// Avoid verbose simulation information
    #[arg(short, long, default_value_t = false)]
    quiet: bool,

    /// Number of pool-thread threads used by the header and body processors.
    /// Defaults to the number of logical CPU cores.
    #[arg(short, long)]
    processors_threads: Option<usize>,

    /// Number of pool-thread threads used by the virtual processor (for parallel transaction verification).
    /// Defaults to the number of logical CPU cores.
    #[arg(short, long)]
    virtual_threads: Option<usize>,

    /// Logging level for all subsystems {off, error, warn, info, debug, trace}
    ///  -- You may also specify <subsystem>=<level>,<subsystem2>=<level>,... to set the log level for individual subsystems
    #[arg(long = "loglevel", default_value = "info")]
    log_level: String,

    /// Output directory to save the simulation DB
    #[arg(short, long)]
    output_dir: Option<String>,

    /// Input directory of a previous simulation DB (NOTE: simulation args must be compatible with the original run)
    #[arg(short, long)]
    input_dir: Option<String>,
}

/// Calculates the k parameter of the GHOSTDAG protocol such that anticones lager than k will be created
/// with probability less than `delta` (follows eq. 1 from section 4.2 of the PHANTOM paper)
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
    kaspa_core::log::init_logger(&args.log_level);
    assert!(args.bps * args.delay < 250.0, "The delay times bps product is larger than 250");
    if args.miners > 1 {
        println!(
            "Warning: number of miners was configured to {}. Currently each miner added doubles the simulation 
        memory and runtime footprint, while a single miner is sufficient for most simulation purposes (delay is simulated anyway).",
            args.miners
        );
    }
    let mut params = DEVNET_PARAMS.clone_with_skip_pow();
    let mut perf_params = PERF_PARAMS;
    adjust_consensus_params(&args, &mut params);
    adjust_perf_params(&args, &params, &mut perf_params);

    // Load an existing consensus or run the simulation
    let (consensus, _lifetime) = if let Some(input_dir) = args.input_dir {
        let (lifetime, db) = load_existing_db(input_dir);
        let consensus = Arc::new(Consensus::with_perf_params(db, &params, &perf_params));
        (consensus, lifetime)
    } else {
        let until = if args.target_blocks.is_none() { args.sim_time * 1000 } else { u64::MAX }; // milliseconds
        let mut sim = KaspaNetworkSimulator::new(args.delay, args.bps, args.target_blocks, &params, &perf_params, args.output_dir);
        let (consensus, handles, lifetime) = sim.init(args.miners, args.tpb, !args.quiet).run(until);
        consensus.shutdown(handles);
        (consensus, lifetime)
    };

    // Benchmark the DAG validation time
    let (_lifetime2, db2) = create_temp_db();
    let consensus2 = Arc::new(Consensus::with_perf_params(db2, &params, &perf_params));
    let handles2 = consensus2.init();
    validate(&consensus, &consensus2, &params, args.delay, args.bps);
    consensus2.shutdown(handles2);
    drop(consensus);
}

fn adjust_consensus_params(args: &Args, params: &mut Params) {
    if args.bps * args.delay > 2.0 {
        let k = u64::max(calculate_ghostdag_k(2.0 * args.delay * args.bps, 0.05), params.ghostdag_k as u64);
        let k = u64::min(k, KType::MAX as u64) as KType; // Clamp to KType::MAX
        params.ghostdag_k = k;
        params.mergeset_size_limit = k as u64 * 10;
        params.max_block_parents = u8::max((0.66 * k as f64) as u8, 10);
        params.target_time_per_block = (1000.0 / args.bps) as u64;
        params.merge_depth = (params.merge_depth as f64 * args.bps) as u64;
        params.coinbase_maturity = (params.coinbase_maturity as f64 * f64::max(1.0, args.bps * args.delay * 0.25)) as u64;
        params.difficulty_window_size = (params.difficulty_window_size as f64 * args.bps) as usize; // Scale the DAA window linearly with BPS

        println!(
            "The delay times bps product is larger than 2 (2Dλ={}), setting GHOSTDAG K={}, DAA window size={}",
            2.0 * args.delay * args.bps,
            k,
            params.difficulty_window_size
        );
    }
}

fn adjust_perf_params(args: &Args, consensus_params: &Params, perf_params: &mut PerfParams) {
    // Allow caching up to ~2000 full blocks
    perf_params.block_data_cache_size = (perf_params.block_data_cache_size as f64 * args.bps.clamp(1.0, 10.0)) as u64;

    let daa_window_memory_budget = 1_000_000_000u64; // 1GB
    let single_window_byte_size = consensus_params.difficulty_window_size as u64 * size_of::<SortableBlock>() as u64;
    let max_daa_window_cache_size = daa_window_memory_budget / single_window_byte_size;
    perf_params.block_window_cache_size = u64::min(perf_params.block_window_cache_size, max_daa_window_cache_size);

    let headers_memory_budget = 1_000_000_000u64; // 1GB
    let approx_header_num_parents = (args.bps * args.delay) as u64 * 2; // x2 for multi-levels
    let approx_header_byte_size = approx_header_num_parents * size_of::<Hash>() as u64 + size_of::<Header>() as u64;
    let max_headers_cache_size = headers_memory_budget / approx_header_byte_size;
    perf_params.header_data_cache_size = u64::min(perf_params.header_data_cache_size, max_headers_cache_size);

    if let Some(processors_pool_threads) = args.processors_threads {
        perf_params.block_processors_num_threads = processors_pool_threads;
    }
    if let Some(virtual_pool_threads) = args.virtual_threads {
        perf_params.virtual_processor_num_threads = virtual_pool_threads;
    }
}

#[tokio::main]
async fn validate(src_consensus: &Consensus, dst_consensus: &Consensus, params: &Params, delay: f64, bps: f64) {
    let hashes = topologically_ordered_hashes(src_consensus, params.genesis_hash);
    let num_blocks = hashes.len();
    let num_txs = print_stats(src_consensus, &hashes, delay, bps, params.ghostdag_k);
    println!("Validating {} blocks with {} transactions overall...", num_blocks, num_txs);
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
    let elapsed = start.elapsed();
    println!(
        "Total validation time: {:?}, block processing rate: {:.2} (b/s), transaction processing rate: {:.2} (t/s)",
        elapsed,
        num_blocks as f64 / elapsed.as_secs_f64(),
        num_txs as f64 / elapsed.as_secs_f64(),
    );
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

fn print_stats(src_consensus: &Consensus, hashes: &[Hash], delay: f64, bps: f64, k: KType) -> usize {
    let blues_mean = hashes.iter().map(|&h| src_consensus.ghostdag_store.get_data(h).unwrap().mergeset_blues.len()).sum::<usize>()
        as f64
        / hashes.len() as f64;
    let reds_mean = hashes.iter().map(|&h| src_consensus.ghostdag_store.get_data(h).unwrap().mergeset_reds.len()).sum::<usize>()
        as f64
        / hashes.len() as f64;
    let parents_mean = hashes.iter().map(|&h| src_consensus.headers_store.get_header(h).unwrap().direct_parents().len()).sum::<usize>()
        as f64
        / hashes.len() as f64;
    let num_txs = hashes.iter().map(|&h| src_consensus.block_transactions_store.get(h).unwrap().len()).sum::<usize>();
    let txs_mean = num_txs as f64 / hashes.len() as f64;
    println!("[DELAY={}, BPS={}, GHOSTDAG K={}]", delay, bps, k);
    println!(
        "[Average stats of generated DAG] blues: {}, reds: {}, parents: {}, txs: {}",
        blues_mean, reds_mean, parents_mean, txs_mean
    );
    num_txs
}
