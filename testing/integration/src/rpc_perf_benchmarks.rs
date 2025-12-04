use crate::{
    common::{
        args::ArgsBuilder,
        daemon::{ClientManager, Daemon},
        utils::{generate_tx_dag, verify_tx_dag, CONTRACT_FACTOR},
    },
    tasks::{block::group::MinerGroupTask, daemon::DaemonTask, tx::group::TxSenderGroupTask, Stopper, TasksRunner},
};
use futures_util::future::join_all;
use kaspa_addresses::Address;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::params::Params;
use kaspa_consensus_core::{constants::SOMPI_PER_KASPA, network::NetworkType};
use kaspa_core::info;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_txscript::pay_to_address_script;
use rand::thread_rng;
use rand::Rng;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const SUBMIT_BLOCK_CLIENTS: usize = 2;
const BLOCK_COUNT: usize = 100_000;

// Constants for transaction generation and mempool pressure
const MEMPOOL_TARGET: u64 = 20_000;
const TX_COUNT: usize = 100_000;
const TX_LEVEL_WIDTH: usize = 5_000;
const TPS_PRESSURE: u64 = 10;
const PREALLOC_AMOUNT_SOMPI: u64 = 1;
const SUBMIT_TX_CLIENTS: usize = 2;

/// `cargo test --release --package kaspa-testing-integration --lib --features devnet-prealloc -- rpc_perf_benchmarks::bench_rpc_high_load --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
#[cfg(feature = "devnet-prealloc")] // Add this feature gate
async fn bench_rpc_high_load() {
    use tokio::time::sleep;

    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("info,kaspa_core::time=debug,kaspa_mining::monitor=debug");
    kaspa_core::panic::configure_panic();

    // Setup for pre-allocated UTXOs and transaction generation
    let (prealloc_sk, prealloc_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let prealloc_address =
        Address::new(NetworkType::Simnet.into(), kaspa_addresses::Version::PubKey, &prealloc_pk.x_only_public_key().0.serialize());
    let schnorr_key = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &prealloc_sk);
    let spk = pay_to_address_script(&prealloc_address);

    let args = ArgsBuilder::simnet(TX_LEVEL_WIDTH as u64 * CONTRACT_FACTOR, PREALLOC_AMOUNT_SOMPI) // Use simnet with prealloc args
        .prealloc_address(prealloc_address.clone()) // Set prealloc address
        .apply_args(Daemon::fill_args_with_random_ports)
        .utxoindex(true) // Ensure utxoindex is enabled for transaction validation
        .build();

    let network = args.network();
    let params: Params = network.into();

    // Generate UTXOs from args
    let utxoset = args.generate_prealloc_utxos(args.num_prealloc_utxos.unwrap());
    let txs = generate_tx_dag(utxoset.clone(), schnorr_key, spk, TX_COUNT / TX_LEVEL_WIDTH, TX_LEVEL_WIDTH);
    verify_tx_dag(&utxoset, &txs);
    info!("Generated overall {} txs for mempool pressure.", txs.len());

    let client_manager = Arc::new(ClientManager::new(args));

    let mut tasks = TasksRunner::new(Some(DaemonTask::build(client_manager.clone()))).launch().await;

    // Continuous mining
    tasks = tasks.task(
        MinerGroupTask::build(
            network,
            client_manager.clone(),
            SUBMIT_BLOCK_CLIENTS,
            params.bps().upper_bound(),
            BLOCK_COUNT,
            Stopper::Signal,
        )
        .await,
    );

    // Transaction generator/simulator
    tasks = tasks.task(
        TxSenderGroupTask::build(client_manager.clone(), SUBMIT_TX_CLIENTS, false, txs, TPS_PRESSURE, MEMPOOL_TARGET, Stopper::Signal)
            .await,
    );

    tasks.run().await;

    sleep(Duration::from_secs(5)).await;

    let main_client = client_manager.new_client().await;
    let dag_info = main_client.get_block_dag_info().await.unwrap();
    let sink = dag_info.sink;

    info!("Waiting 5 seconds before starting...");

    sleep(Duration::from_secs(2)).await;

    let initial_virtual_chain = main_client.get_virtual_chain_from_block(sink, false, None).await.unwrap().added_chain_block_hashes;

    // High load RPC simulation
    info!("Starting high load RPC simulation...");

    let num_clients = 100;
    let num_requests_per_client = 100;

    let start_total = Instant::now();

    let mut handles = Vec::new();
    for _ in 0..num_clients {
        let client = client_manager.new_client().await;
        let thread_virtual_chain = initial_virtual_chain.clone();
        let handle = tokio::spawn(async move {
            let mut latencies = Vec::with_capacity(num_requests_per_client);
            for _ in 0..num_requests_per_client {
                let index = rand::thread_rng().gen_range(0..(thread_virtual_chain.len() - 1));
                let start = Instant::now();

                let hash = thread_virtual_chain.get(index).unwrap();

                let vspcv2_response = client
                    .get_virtual_chain_from_block_v2(*hash, Some(kaspa_rpc_core::RpcDataVerbosityLevel::High), None)
                    .await
                    .unwrap();
                info!(
                    "{} - {} - {}",
                    vspcv2_response.added_chain_block_hashes.len(),
                    vspcv2_response.removed_chain_block_hashes.len(),
                    vspcv2_response.chain_block_accepted_transactions.len()
                );
                latencies.push(start.elapsed());
            }
            client.disconnect().await.unwrap();
            latencies
        });
        handles.push(handle);
    }

    let results = join_all(handles).await;
    let total_duration = start_total.elapsed();

    let mut all_latencies: Vec<_> = results.into_iter().flat_map(|res| res.unwrap()).collect();
    all_latencies.sort_unstable();

    let total_requests = all_latencies.len();
    if total_requests == 0 {
        info!("No requests were made.");
    } else {
        let rps = total_requests as f64 / total_duration.as_secs_f64();
        let avg_latency: Duration = all_latencies.iter().sum::<Duration>() / total_requests as u32;
        let min_latency = all_latencies.first().unwrap();
        let max_latency = all_latencies.last().unwrap();
        let p95_index = ((0.95 * total_requests as f64).ceil() as usize).saturating_sub(1);
        let p99_index = ((0.99 * total_requests as f64).ceil() as usize).saturating_sub(1);
        let p95_latency = all_latencies[p95_index];
        let p99_latency = all_latencies[p99_index];

        info!("Finished high load simulation.");
        info!("Total requests: {}", total_requests);
        info!("Total duration: {:?}", total_duration);
        info!("Requests per second: {:.2}", rps);
        info!("--------------------");
        info!("Latency metrics:");
        info!("  Min: {:?}", min_latency);
        info!("  Max: {:?}", max_latency);
        info!("  Avg: {:?}", avg_latency);
        info!("  p95: {:?}", p95_latency);
        info!("  p99: {:?}", p99_latency);
    }

    // Fold-up
    tasks.join().await;
}
