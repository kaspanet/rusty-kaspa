//! Aero solo miner: fetches block templates from an `aerod` node over gRPC, searches for a RandomX nonce on
//! `--threads` threads, and submits any block that meets the target.
//!
//! By default it mines in RandomX *fast* mode, which builds a ~2 GiB dataset once at startup (it needs roughly
//! 2.5 GiB of free RAM). Pass `--light` to use the 256 MiB cache instead on small machines: far slower per hash,
//! but the hashes are identical.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use aero_addresses::Address;
use aero_consensus_core::header::Header;
use aero_core::{info, warn};
use aero_grpc_client::GrpcClient;
use aero_notify::subscription::context::SubscriptionContext;
use aero_pow::{Hasher, State};
use aero_rpc_core::{RpcRawBlock, api::rpc::RpcApi, notify::mode::NotificationMode};
use clap::Parser;
use parking_lot::Mutex;
use rand::Rng;
use tokio::sync::mpsc;

/// Hashes between checks for fresher work. Small, so a new template is picked up within a few hashes.
const HASHES_PER_WORK_CHECK: usize = 4;

#[derive(Parser)]
#[command(name = "aero-miner", about = "Aero RandomX solo miner")]
struct Args {
    /// gRPC address (host:port) of the aerod node to mine for
    #[arg(short = 's', long, default_value = "127.0.0.1:16110")]
    rpcserver: String,

    /// Address that receives the block rewards
    #[arg(short = 'a', long)]
    mining_address: String,

    /// Number of mining threads (default: all logical CPUs)
    #[arg(short = 't', long)]
    threads: Option<usize>,

    /// Use RandomX light mode (256 MiB) instead of fast mode (~2 GiB dataset)
    #[arg(long)]
    light: bool,

    /// How often to fetch a fresh block template, in milliseconds
    #[arg(long, default_value_t = 250)]
    poll_ms: u64,

    /// Keep mining even if the node reports that it is not synced
    #[arg(long)]
    mine_unsynced: bool,
}

/// One block template and the PoW state derived from it.
struct Job {
    id: u64,
    block: RpcRawBlock,
    state: State,
}

struct Shared {
    job: Mutex<Option<Arc<Job>>>,
    /// Id of the newest job, so workers can cheaply detect new work without taking the lock.
    latest_job_id: AtomicU64,
    /// Id of the newest job that already has a solution, so we stop hashing it and wait for fresh work instead of
    /// submitting a pile of sibling blocks for the same template (which bloats the DAG, most visibly at low difficulty).
    solved_job_id: AtomicU64,
    stop: AtomicBool,
    hashes: AtomicU64,
}

fn make_job(id: u64, block: RpcRawBlock) -> Result<Job, String> {
    let header: Header = (&block.header).try_into().map_err(|e| format!("invalid block template header: {e:?}"))?;
    Ok(Job { id, state: State::new(&header), block })
}

fn worker(shared: Arc<Shared>, light: bool, found: mpsc::UnboundedSender<(Arc<Job>, u64)>) {
    let mut hasher = if light { Hasher::light() } else { Hasher::fast() };
    let mut rng = rand::thread_rng();

    let mut job: Option<Arc<Job>> = None;
    let mut nonce = 0u64;
    while !shared.stop.load(Ordering::Relaxed) {
        let newest = shared.latest_job_id.load(Ordering::Acquire);
        if job.as_ref().is_none_or(|j| j.id != newest) {
            match shared.job.lock().clone() {
                Some(fresh) => {
                    job = Some(fresh);
                    // A random starting point keeps threads (and restarts) from repeating each other's work.
                    nonce = rng.r#gen();
                }
                None => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
            }
        }

        let current = job.as_ref().expect("job was just set");
        if shared.solved_job_id.load(Ordering::Relaxed) == current.id {
            // This template is solved; wait for the next one.
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        for _ in 0..HASHES_PER_WORK_CHECK {
            let (passed, _) = current.state.check_pow_with(&mut hasher, nonce);
            if passed {
                // Only the first solution for a template is submitted; the others are discarded.
                if shared.solved_job_id.swap(current.id, Ordering::AcqRel) != current.id {
                    let _ = found.send((Arc::clone(current), nonce));
                }
                break;
            }
            nonce = nonce.wrapping_add(1);
        }
        shared.hashes.fetch_add(HASHES_PER_WORK_CHECK as u64, Ordering::Relaxed);
    }
}

async fn submit(client: &GrpcClient, job: &Job, nonce: u64) {
    let mut block = job.block.clone();
    block.header.nonce = nonce;
    match client.submit_block(block, false).await {
        Ok(response) if response.report.is_success() => info!("Block found and accepted by the node (nonce {nonce})"),
        Ok(response) => warn!("Block found but rejected by the node: {:?}", response.report),
        Err(e) => warn!("Block found but submit_block failed: {e}"),
    }
}

#[tokio::main]
async fn main() {
    aero_core::log::init_logger(None, "");
    let args = Args::parse();

    let pay_address = Address::try_from(args.mining_address.as_str()).expect("invalid --mining-address");
    let threads = args.threads.unwrap_or_else(|| thread::available_parallelism().map_or(1, |n| n.get())).max(1);

    // Build the shared RandomX state up front, so the first template isn't wasted waiting on it.
    if args.light {
        info!("Initializing the RandomX cache (256 MiB)...");
        tokio::task::spawn_blocking(|| drop(aero_pow::cache())).await.expect("cache initialization panicked");
    } else {
        info!("Building the RandomX dataset (~2 GiB). This takes a while, then mining is much faster per hash...");
        tokio::task::spawn_blocking(|| drop(aero_pow::dataset())).await.expect("dataset build panicked");
    }

    info!("Connecting to grpc://{}", args.rpcserver);
    let client = GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{}", args.rpcserver),
        Some(SubscriptionContext::new()),
        true,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .expect("failed to connect to the node's gRPC server");
    info!("Connected. Mining to {} with {threads} thread(s), {} mode", pay_address, if args.light { "light" } else { "fast" });

    let shared = Arc::new(Shared {
        job: Mutex::new(None),
        latest_job_id: AtomicU64::new(0),
        solved_job_id: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        hashes: AtomicU64::new(0),
    });
    let (found_tx, mut found_rx) = mpsc::unbounded_channel::<(Arc<Job>, u64)>();
    let workers: Vec<_> = (0..threads)
        .map(|_| {
            let (shared, found_tx) = (Arc::clone(&shared), found_tx.clone());
            thread::spawn(move || worker(shared, args.light, found_tx))
        })
        .collect();
    drop(found_tx);

    let mut template_tick = tokio::time::interval(Duration::from_millis(args.poll_ms.max(10)));
    template_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut stats_tick = tokio::time::interval(Duration::from_secs(10));
    let (mut last_hashes, mut last_stats) = (0u64, Instant::now());
    let mut next_id = 1u64;

    loop {
        tokio::select! {
            _ = template_tick.tick() => {
                match client.get_block_template(pay_address.clone(), vec![]).await {
                    Ok(template) if !template.is_synced && !args.mine_unsynced => {
                        warn!("Node is not synced; waiting (use --mine-unsynced to override)");
                        *shared.job.lock() = None;
                    }
                    Ok(template) => match make_job(next_id, template.block) {
                        Ok(job) => {
                            *shared.job.lock() = Some(Arc::new(job));
                            shared.latest_job_id.store(next_id, Ordering::Release);
                            next_id += 1;
                        }
                        Err(e) => warn!("{e}"),
                    },
                    Err(e) => warn!("get_block_template failed: {e}"),
                }
            }
            Some((job, nonce)) = found_rx.recv() => {
                submit(&client, &job, nonce).await;
            }
            _ = stats_tick.tick() => {
                let hashes = shared.hashes.load(Ordering::Relaxed);
                let secs = last_stats.elapsed().as_secs_f64().max(0.001);
                info!("Hashrate: {:.1} H/s", (hashes - last_hashes) as f64 / secs);
                (last_hashes, last_stats) = (hashes, Instant::now());
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down...");
                break;
            }
        }
    }

    shared.stop.store(true, Ordering::Relaxed);
    for handle in workers {
        let _ = handle.join();
    }
}
