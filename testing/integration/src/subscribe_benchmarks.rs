use crate::common::{
    self,
    args::ArgsBuilder,
    daemon::{ClientManager, Daemon},
    tasks::{
        block::full::FullMinerTask,
        daemon::{DaemonArgs, DaemonTask},
        memory_monitor::MemoryMonitorTask,
        stat_recorder::StatRecorderTask,
        subscription::full::FullSubscriberTask,
        tick::TickTask,
        tx::full::FullTxSenderTask,
        TasksRunner,
    },
    utils::CONTRACT_FACTOR,
};
use clap::Parser;
use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_core::{info, task::tick::TickService, trace};
use kaspa_math::Uint256;
use kaspa_notify::{address::tracker::Indexes, scope::VirtualDaaScoreChangedScope};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_txscript::pay_to_address_script;
use rand::thread_rng;
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use workflow_perf_monitor::mem::get_process_memory_info;

// Constants
const BLOCK_COUNT: usize = usize::MAX;

const MEMPOOL_TARGET: u64 = 650;
const TX_COUNT: usize = 1_500_000;
const TX_LEVEL_WIDTH: usize = 20_000;
const TPS_PRESSURE: u64 = 150; // 100
const PREALLOC_AMOUNT: u64 = 500;

const SUBMIT_BLOCK_CLIENTS: usize = 20;
const SUBMIT_TX_CLIENTS: usize = 1;
const SUBSCRIBE_WORKERS: usize = 20;

#[cfg(feature = "heap")]
const MAX_MEMORY: u64 = 22_000_000_000;
#[cfg(not(feature = "heap"))]
const MAX_MEMORY: u64 = 31_000_000_000;

const NOTIFY_CLIENTS: usize = 500;
const MAX_ADDRESSES: usize = 1_000_000;
const WALLET_ADDRESSES: usize = 800;

const STAT_FOLDER: &'static str = "../../../analyze/mem-logs";

fn create_client_addresses(index: usize, network_id: &NetworkId) -> Vec<Address> {
    // Process in heaviest to lightest requests order, maximizing messages memory footprint
    // between notifiers and from notifier to broadcasters at grpc server and rpc core levels
    let max_address = ((NOTIFY_CLIENTS - index) * MAX_ADDRESSES / NOTIFY_CLIENTS) + 1;
    let min_address = if (NOTIFY_CLIENTS - index) % (NOTIFY_CLIENTS / 5) == 0 {
        // Create a typical UTXOs monitoring service subscription scope
        0
    } else {
        // Create a typical wallet subscription scope
        max_address.max(WALLET_ADDRESSES) - WALLET_ADDRESSES
    };
    (min_address..max_address)
        .map(|x| Address::new((*network_id).into(), kaspa_addresses::Version::PubKey, &Uint256::from_u64(x as u64).to_le_bytes()))
        .collect_vec()
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_demo_child_process --exact --nocapture --ignored -- --rpc=16610 --p2p=16611`
#[tokio::test]
#[ignore = "demo"]
async fn bench_demo_child_process() {
    init_allocator_with_default_settings();
    kaspa_core::panic::configure_panic();
    kaspa_core::log::try_init_logger(
        "INFO, kaspa_core::time=debug, kaspa_rpc_core=debug, kaspa_grpc_client=debug, kaspa_notify=info, kaspa_notify::address::tracker=debug, kaspa_notify::listener=debug, kaspa_notify::subscription::single=debug, kaspa_mining::monitor=debug, kaspa_testing_integration::subscribe_benchmarks=trace", 
    );

    let args = DaemonArgs::from_env_args();
    trace!("RPC port: {}", args.rpc);
    trace!("P2P port: {}", args.p2p);

    let cli_args: Vec<String> = std::env::args().collect();
    for (i, arg) in cli_args.iter().enumerate() {
        info!("arg {} = {}", i, arg);
    }
    let before = get_process_memory_info().unwrap();
    let mut store = vec![];
    for _ in 0..10 {
        store.push(Indexes::with_capacity(10_000_000));
        let after = get_process_memory_info().unwrap();
        trace!("Child memory consumed: {}", (after.resident_set_size - before.resident_set_size));
        sleep(Duration::from_secs(1)).await;
    }
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc --profile release -- subscribe_benchmarks::bench_demo_parent_process --exact --nocapture --ignored`
///
/// Simple demo test of a parent process launching a child process both living in total isolation,
/// notably having separate independent memory spaces but both logging to the same console.
#[tokio::test]
#[ignore = "demo"]
async fn bench_demo_parent_process() {
    init_allocator_with_default_settings();
    kaspa_core::panic::configure_panic();
    kaspa_core::log::try_init_logger(
        "INFO, kaspa_core::time=debug, kaspa_rpc_core=debug, kaspa_grpc_client=debug, kaspa_notify=info, kaspa_notify::address::tracker=debug, kaspa_notify::listener=debug, kaspa_notify::subscription::single=debug, kaspa_mining::monitor=debug, kaspa_testing_integration::subscribe_benchmarks=trace", 
    );
    let parent_args =
        DaemonArgs::new(16610, 16611, "c1577399734a1f8a96cfa6b64facb7d52d51c44fa03d03bcfef0e3ed9b7f9cad".to_owned(), None);
    let args = parent_args.to_command_args("subscribe_benchmarks::bench_demo_child_process");
    let before = get_process_memory_info().unwrap();
    trace!("Launching child process...");
    let mut server = tokio::process::Command::new("cargo").args(args).spawn().expect("failed to start child process");

    for _ in 0..10 {
        let after = get_process_memory_info().unwrap();
        trace!("Parent memory consumed: {}", (after.resident_set_size - before.resident_set_size));
        sleep(Duration::from_secs(1)).await;
    }

    trace!("Waiting for child process to exit...");
    server.wait().await.expect("failed to wait for child process");
}

#[test]
fn test_daemon_args() {
    kaspa_core::log::try_init_logger("trace");
    let args = vec![
        "test",
        "--rpc",
        "16610",
        "--p2p",
        "16611",
        "--private-key",
        "c1577399734a1f8a96cfa6b64facb7d52d51c44fa03d03bcfef0e3ed9b7f9cad",
        "--stat-file-prefix",
        "demo",
    ];

    let daemon_args = DaemonArgs::parse_from(args);
    trace!("RPC port: {}", daemon_args.rpc);
    trace!("P2P port: {}", daemon_args.p2p);
    trace!("Private key: {}", daemon_args.private_key);
    trace!("Stat file prefix: {}", daemon_args.stat_file_prefix.unwrap());

    let args = vec!["test"];

    let daemon_args = DaemonArgs::try_parse_from(args);
    assert!(daemon_args.is_err());
}

#[test]
fn test_keys() {
    kaspa_core::log::try_init_logger("trace");
    let (prealloc_sk, prealloc_pk) = secp256k1::generate_keypair(&mut thread_rng());

    let key_pair = secp256k1::KeyPair::from_secret_key(secp256k1::SECP256K1, &prealloc_sk);
    assert_eq!(key_pair.public_key(), prealloc_pk);
    assert_eq!(key_pair.secret_key(), prealloc_sk);

    let secret_key_hex = prealloc_sk.display_secret().to_string();
    trace!("Private key = {}", secret_key_hex);

    let mut private_key_bytes = [0u8; 32];
    faster_hex::hex_decode(secret_key_hex.as_bytes(), &mut private_key_bytes).unwrap();
    let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).unwrap();
    assert_eq!(schnorr_key.public_key(), prealloc_pk);
    assert_eq!(schnorr_key.secret_key(), prealloc_sk);
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::utxos_changed_subscriptions_sanity_check --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn utxos_changed_subscriptions_sanity_check() {
    init_allocator_with_default_settings();
    kaspa_core::panic::configure_panic();
    kaspa_core::log::try_init_logger(
        "INFO, kaspa_core::time=debug, kaspa_rpc_core=debug, kaspa_grpc_client=debug, kaspa_notify=info, kaspa_notify::address::tracker=debug, kaspa_notify::listener=debug, kaspa_notify::subscription::single=debug, kaspa_mining::monitor=debug, kaspa_testing_integration::subscribe_benchmarks=trace", 
    );

    //
    // Setup
    //
    let (prealloc_sk, _) = secp256k1::generate_keypair(&mut thread_rng());
    let args = ArgsBuilder::simnet(TX_LEVEL_WIDTH as u64 * CONTRACT_FACTOR, PREALLOC_AMOUNT)
        .apply_args(|args| Daemon::fill_args_with_random_ports(args))
        .build();

    // Start the daemon
    info!("Launching the daemon...");
    let daemon_args = DaemonArgs::new(
        args.rpclisten.map(|x| x.normalize(0).port).unwrap(),
        args.listen.map(|x| x.normalize(0).port).unwrap(),
        prealloc_sk.display_secret().to_string(),
        Some("ucs-server".to_owned()),
    );
    let server_start_time = std::time::Instant::now();
    let mut daemon_process = tokio::process::Command::new("cargo")
        .args(daemon_args.to_command_args("subscribe_benchmarks::bench_utxos_changed_subscriptions_daemon"))
        .spawn()
        .expect("failed to start daemon process");

    // Make sure that the server was given enough time to start
    let client_start_time = server_start_time + Duration::from_secs(5);
    if client_start_time > std::time::Instant::now() {
        tokio::time::sleep(client_start_time - std::time::Instant::now()).await;
    }

    // Initial objects
    let client_manager = Arc::new(ClientManager::new(args));
    let client = client_manager.new_client().await;

    //
    // Fold-up
    //
    kaspa_core::info!("Signal the daemon to shutdown");
    client.shutdown().await.unwrap();
    kaspa_core::warn!("Disconnect the main client");
    client.disconnect().await.unwrap();
    drop(client);

    kaspa_core::warn!("Waiting for the daemon to exit...");
    daemon_process.wait().await.expect("failed to wait for the daemon process");
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_daemon --exact --nocapture --ignored -- --rpc=16610 --p2p=16611`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_daemon() {
    init_allocator_with_default_settings();
    kaspa_core::panic::configure_panic();
    kaspa_core::log::try_init_logger(
        "INFO, kaspa_core::core=trace, kaspa_core::time=debug, kaspa_rpc_core=debug, kaspa_grpc_client=debug, kaspa_notify=info, kaspa_notify::address::tracker=debug, kaspa_notify::listener=debug, kaspa_notify::subscription::single=debug, kaspa_mining::monitor=debug, kaspa_testing_integration::subscribe_benchmarks=trace", 
    );

    let daemon_args = DaemonArgs::from_env_args();
    let args = ArgsBuilder::simnet(TX_LEVEL_WIDTH as u64 * CONTRACT_FACTOR, PREALLOC_AMOUNT).apply_daemon_args(&daemon_args).build();
    let tick_service = Arc::new(TickService::new());

    let mut tasks = TasksRunner::new(Some(DaemonTask::with_args(args.clone())))
        .task(TickTask::build(tick_service.clone()))
        .task(MemoryMonitorTask::build(tick_service, "daemon", Duration::from_secs(5), MAX_MEMORY))
        .optional_task(StatRecorderTask::optional(
            Duration::from_secs(5),
            STAT_FOLDER.to_owned(),
            daemon_args.stat_file_prefix.clone(),
            true,
        ));
    tasks.run();
    tasks.join().await;

    trace!("Daemon was successfully shut down");
}

async fn utxos_changed_subscriptions_client(address_cycle_seconds: u64, address_max_cycles: usize) {
    init_allocator_with_default_settings();
    kaspa_core::panic::configure_panic();
    kaspa_core::log::try_init_logger(
        "INFO, kaspa_core::time=debug, kaspa_rpc_core=debug, kaspa_grpc_client=debug, kaspa_notify=info, kaspa_notify::address::tracker=debug, kaspa_notify::listener=debug, kaspa_notify::subscription::single=debug, kaspa_mining::monitor=debug, kaspa_testing_integration::subscribe_benchmarks=trace", 
    );

    assert!(address_cycle_seconds >= 60);
    if TX_COUNT < TX_LEVEL_WIDTH {
        panic!()
    }

    //
    // Setup
    //
    let (prealloc_sk, prealloc_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let prealloc_address =
        Address::new(NetworkType::Simnet.into(), kaspa_addresses::Version::PubKey, &prealloc_pk.x_only_public_key().0.serialize());
    let schnorr_key = secp256k1::KeyPair::from_secret_key(secp256k1::SECP256K1, &prealloc_sk);
    let spk = pay_to_address_script(&prealloc_address);

    let args = ArgsBuilder::simnet(TX_LEVEL_WIDTH as u64 * CONTRACT_FACTOR, PREALLOC_AMOUNT)
        .prealloc_address(prealloc_address)
        .apply_args(|args| Daemon::fill_args_with_random_ports(args))
        .build();
    let network = args.network();
    let params: Params = network.into();

    let utxoset = args.generate_prealloc_utxos(args.num_prealloc_utxos.unwrap());
    let txs = common::utils::generate_tx_dag(
        utxoset.clone(),
        schnorr_key,
        spk,
        (TX_COUNT + TX_LEVEL_WIDTH - 1) / TX_LEVEL_WIDTH,
        TX_LEVEL_WIDTH,
    );
    common::utils::verify_tx_dag(&utxoset, &txs);
    info!("Generated overall {} txs", txs.len());

    // Start the daemon
    info!("Launching the daemon...");
    let daemon_args = DaemonArgs::new(
        args.rpclisten.map(|x| x.normalize(0).port).unwrap(),
        args.listen.map(|x| x.normalize(0).port).unwrap(),
        prealloc_sk.display_secret().to_string(),
        Some("ucs-server".to_owned()),
    );
    let server_start_time = std::time::Instant::now();
    let mut daemon_process = tokio::process::Command::new("cargo")
        .args(daemon_args.to_command_args("subscribe_benchmarks::bench_utxos_changed_subscriptions_daemon"))
        .spawn()
        .expect("failed to start daemon process");

    // Make sure that the server was given enough time to start
    let client_start_time = server_start_time + Duration::from_secs(5);
    if client_start_time > std::time::Instant::now() {
        tokio::time::sleep(client_start_time - std::time::Instant::now()).await;
    }

    // Initial objects
    let subscribing_addresses = (0..NOTIFY_CLIENTS).map(|i| Arc::new(create_client_addresses(i, &params.net))).collect_vec();
    let client_manager = Arc::new(ClientManager::new(args));
    let client = client_manager.new_client().await;
    let tick_service = Arc::new(TickService::new());

    let mut tasks = TasksRunner::new(None)
        .task(TickTask::build(tick_service.clone()))
        .task(MemoryMonitorTask::build(tick_service.clone(), "client", Duration::from_secs(5), MAX_MEMORY))
        .task(FullMinerTask::build(network, client_manager.clone(), SUBMIT_BLOCK_CLIENTS, params.bps(), BLOCK_COUNT).await)
        .task(FullTxSenderTask::build(client_manager.clone(), SUBMIT_TX_CLIENTS, true, txs, TPS_PRESSURE, MEMPOOL_TARGET).await)
        .task(
            FullSubscriberTask::build(
                client_manager,
                SUBSCRIBE_WORKERS,
                params.bps(),
                vec![VirtualDaaScoreChangedScope {}.into()],
                3,
                subscribing_addresses,
                5,
                address_cycle_seconds,
                address_max_cycles,
            )
            .await,
        );
    tasks.run();
    tasks.join().await;

    //
    // Fold-up
    //
    kaspa_core::info!("Signal the daemon to shutdown");
    client.shutdown().await.unwrap();
    kaspa_core::warn!("Disconnect the main client");
    client.disconnect().await.unwrap();
    drop(client);

    kaspa_core::warn!("Waiting for the daemon to exit...");
    daemon_process.wait().await.expect("failed to wait for the daemon process");
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_a --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_a() {
    // No subscriptions
    utxos_changed_subscriptions_client(1200, 0).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_b --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_b() {
    // Single initial subscriptions, no cycles
    utxos_changed_subscriptions_client(60, 1).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_c --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_c() {
    // 1 hour subscription cycles
    utxos_changed_subscriptions_client(3600, usize::MAX).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_d --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_d() {
    // 20 minutes subscription cycles
    utxos_changed_subscriptions_client(1200, usize::MAX).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_e --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_e() {
    // 3 minutes subscription cycles
    utxos_changed_subscriptions_client(180, usize::MAX).await;
}
