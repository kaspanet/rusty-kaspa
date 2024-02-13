use crate::common::{
    self,
    args::ArgsBuilder,
    client_notify::ChannelNotify,
    daemon::{ClientManager, Daemon},
    tasks::{
        daemon::{DaemonArgs, DaemonTask},
        memory_monitor::MemoryMonitorTask,
        stat_recorder::{stat_recorder_task, StatRecorderTask},
        tick::TickTask,
        TasksRunner,
    },
    utils::CONTRACT_FACTOR,
};
use async_channel::Sender;
use clap::Parser;
use futures_util::future::join_all;
use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::params::Params;
use kaspa_consensus_core::{
    network::{NetworkId, NetworkType},
    tx::Transaction,
};
use kaspa_core::{debug, info, task::tick::TickService, trace, warn};
use kaspa_grpc_client::GrpcClient;
use kaspa_math::Uint256;
use kaspa_notify::{
    address::tracker::Indexes,
    listener::ListenerId,
    scope::{NewBlockTemplateScope, Scope, UtxosChangedScope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::{api::rpc::RpcApi, Notification, RpcBlock, RpcError};
use kaspa_txscript::pay_to_address_script;
use kaspa_utils::channel::Channel;
use parking_lot::Mutex;
use rand::thread_rng;
use rand_distr::{Distribution, Exp};
use std::{
    cmp::max,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{join, sync::oneshot, task::JoinHandle, time::sleep};
use workflow_perf_monitor::mem::get_process_memory_info;

// Constants
const BLOCK_COUNT: usize = usize::MAX;

const MEMPOOL_TARGET: u64 = 1_000; // 10_000
const TX_COUNT: usize = 5_000; //1_500_000;
const TX_LEVEL_WIDTH: usize = 2_000; //20_000;
const TPS_PRESSURE: u64 = 200; // 100
const PREALLOC_AMOUNT: u64 = 500;

const SUBMIT_BLOCK_CLIENTS: usize = 20;
const SUBMIT_TX_CLIENTS: usize = 2;

#[cfg(feature = "heap")]
const MAX_MEMORY: u64 = 22_000_000_000;
#[cfg(not(feature = "heap"))]
const MAX_MEMORY: u64 = 31_000_000_000;

const NOTIFY_CLIENTS: usize = 500;
const MAX_ADDRESSES: usize = 1_000_000;
const WALLET_ADDRESSES: usize = 800;

const STAT_FOLDER: &'static str = "../../../analyze/mem-logs";

struct SubscribingClient {
    pub client: GrpcClient,
    pub addresses: Vec<Address>,
}

impl SubscribingClient {
    fn new(client: GrpcClient, addresses: Vec<Address>) -> Self {
        Self { client, addresses }
    }
}

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

async fn create_subscribing_clients(daemon: &ClientManager) -> Vec<Arc<SubscribingClient>> {
    let clients = daemon.new_clients(NOTIFY_CLIENTS).await;
    clients
        .into_iter()
        .enumerate()
        .map(|(i, client)| Arc::new(SubscribingClient::new(client, create_client_addresses(i, &daemon.network))))
        .collect()
}

enum SubscribeCommand {
    StartBasic(Arc<SubscribingClient>),
    StartUtxosChanged(Arc<SubscribingClient>),
    StopUtxosChanged(Arc<SubscribingClient>),
}

struct SubscriberPool {
    distribution_channel: Channel<SubscribeCommand>,
    feedback_channel: Channel<()>,
    join_handles: Mutex<Option<Vec<JoinHandle<()>>>>,
}

impl SubscriberPool {
    pub fn new(workers: usize, distribution_channel_capacity: usize, params: &Params) -> Self {
        let dist: Exp<f64> = Exp::new(params.bps() as f64).unwrap();
        let distribution_channel = Channel::bounded(distribution_channel_capacity);
        let feedback_channel = Channel::bounded(distribution_channel_capacity);
        let join_handles = (0..workers)
            .into_iter()
            .map(|_| {
                let rx = distribution_channel.receiver();
                let tx = feedback_channel.sender();
                let dist = dist.clone();
                tokio::spawn(async move {
                    while let Ok(command) = rx.recv().await {
                        match command {
                            SubscribeCommand::StartBasic(subscribing_client) => {
                                // subscribing_client.client.start_notify(0, BlockAddedScope {}.into()).await.unwrap();
                                subscribing_client.client.start_notify(0, VirtualDaaScoreChangedScope {}.into()).await.unwrap();
                                // subscribing_client.client
                                //     .start_notify(0, VirtualChainChangedScope { include_accepted_transaction_ids: i % 2 == 0 }.into())
                                //     .await
                                //     .unwrap();
                                tx.send(()).await.unwrap();
                            }
                            SubscribeCommand::StartUtxosChanged(subscribing_client) => loop {
                                match subscribing_client
                                    .client
                                    .start_notify(0, UtxosChangedScope::new(subscribing_client.addresses.clone()).into())
                                    .await
                                {
                                    Ok(_) => {
                                        tx.send(()).await.unwrap();
                                        break;
                                    }
                                    Err(err) => {
                                        warn!(
                                            "Failed to start a subscription with {} addresses: {}",
                                            subscribing_client.addresses.len(),
                                            err
                                        );
                                        let timeout = max((dist.sample(&mut thread_rng()) * 200.0) as u64, 1);
                                        tokio::time::sleep(Duration::from_millis(timeout)).await;
                                    }
                                }
                            },
                            SubscribeCommand::StopUtxosChanged(subscribing_client) => loop {
                                match subscribing_client.client.stop_notify(0, UtxosChangedScope::new(vec![]).into()).await {
                                    Ok(_) => {
                                        tx.send(()).await.unwrap();
                                        break;
                                    }
                                    Err(err) => {
                                        warn!("Failed to stop a subscription: {}", err);
                                        let timeout = max((dist.sample(&mut thread_rng()) * 250.0) as u64, 1);
                                        tokio::time::sleep(Duration::from_millis(timeout)).await;
                                    }
                                }
                            },
                        }
                    }
                })
            })
            .collect();
        let join_handles = Mutex::new(Some(join_handles));

        Self { distribution_channel, feedback_channel, join_handles }
    }

    pub fn sender(&self) -> Sender<SubscribeCommand> {
        self.distribution_channel.sender()
    }

    pub async fn wait_for_feedback(&self, event_count: usize) {
        for _ in 0..event_count {
            self.feedback_channel.recv().await.unwrap();
        }
    }

    pub fn close(&self) {
        self.distribution_channel.close()
    }

    pub fn join_handles(&self) -> Option<Vec<JoinHandle<()>>> {
        self.join_handles.lock().take()
    }
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

    let mut tasks = TasksRunner::new()
        .task(TickTask::build(tick_service.clone()))
        .task(DaemonTask::build(args.clone()))
        .task(MemoryMonitorTask::build(tick_service, "server", Duration::from_secs(5), MAX_MEMORY))
        .optional_task(StatRecorderTask::optional(
            Duration::from_secs(5),
            STAT_FOLDER.to_owned(),
            daemon_args.stat_file_prefix.clone(),
            true,
        ));
    tasks.run();
    tasks.join().await;

    trace!("Server was successfully shut down");
    //utxos_changed_subscriptions_server(ServerArgs::from_env_args()).await;
    // let daemon_args = ServerArgs::new(16610, 16611, "c1577399734a1f8a96cfa6b64facb7d52d51c44fa03d03bcfef0e3ed9b7f9cad".to_owned(), None);
    // utxos_changed_subscriptions_server(daemon_args).await;
}

async fn utxos_changed_subscriptions_client(cycle_seconds: Option<u64>, max_cycles: usize) {
    init_allocator_with_default_settings();
    kaspa_core::panic::configure_panic();
    kaspa_core::log::try_init_logger(
        "INFO, kaspa_core::time=debug, kaspa_rpc_core=debug, kaspa_grpc_client=debug, kaspa_notify=info, kaspa_notify::address::tracker=debug, kaspa_notify::listener=debug, kaspa_notify::subscription::single=debug, kaspa_mining::monitor=debug, kaspa_testing_integration::subscribe_benchmarks=trace", 
    );

    if TX_COUNT < TX_LEVEL_WIDTH {
        panic!()
    }

    let active_utxos_changed_subscriptions = cycle_seconds.is_some();
    let utxos_changed_cycle_seconds: u64 = cycle_seconds.unwrap_or(1200);
    assert!(utxos_changed_cycle_seconds > 20);

    let tick_service = Arc::new(TickService::new());
    let memory_monitor = MemoryMonitorTask::new(tick_service.clone(), "client", Duration::from_secs(1), MAX_MEMORY);
    let memory_monitor_task = memory_monitor.start();

    /*
    Logic:
       1. Use the new feature for preallocating utxos
       2. Set up a dataset with a DAG of signed txs over the preallocated utxoset
       3. Create constant light mempool pressure by submitting txs (via rpc for now)
       4. Mine to the node (simulated)
       5. Connect a set of clients and subscribe them to basic notifications
       6. Alternate bursts of subscriptions and un-subscriptions to UtxosChanged notifications for all clients
       7. Rely on dhat to profile heap memory usage
    */

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

    // Start the server
    info!("Launching the server...");
    let daemon_args = DaemonArgs::new(
        args.rpclisten.map(|x| x.normalize(0).port).unwrap(),
        args.listen.map(|x| x.normalize(0).port).unwrap(),
        prealloc_sk.display_secret().to_string(),
        Some("ucs-server".to_owned()),
    );
    let server_start_time = std::time::Instant::now();
    let mut server_process = tokio::process::Command::new("cargo")
        .args(daemon_args.to_command_args("subscribe_benchmarks::bench_utxos_changed_subscriptions_daemon"))
        .spawn()
        .expect("failed to start child process");

    // Make sure that the server was given enough time to start
    let client_start_time = server_start_time + Duration::from_secs(5);
    if client_start_time > std::time::Instant::now() {
        tokio::time::sleep(client_start_time - std::time::Instant::now()).await;
    }

    let client_manager = ClientManager::new(&args);
    let client = client_manager.new_client().await;
    let bbt_client = client_manager.new_client().await;

    // The time interval between Poisson(lambda) events distributes ~Exp(lambda)
    let dist: Exp<f64> = Exp::new(params.bps() as f64).unwrap();
    let comm_delay = 1000;

    // Mining key and address
    let (sk, pk) = &secp256k1::generate_keypair(&mut thread_rng());
    let pay_address =
        Address::new(network.network_type().into(), kaspa_addresses::Version::PubKey, &pk.x_only_public_key().0.serialize());
    debug!("Generated private key {} and address {}", sk.display_secret(), pay_address);

    let current_template = Arc::new(Mutex::new(bbt_client.get_block_template(pay_address.clone(), vec![]).await.unwrap()));
    let current_template_consume = current_template.clone();

    let executing = Arc::new(AtomicBool::new(true));
    let (bbt_sender, bbt_receiver) = async_channel::unbounded();
    bbt_client.start(Some(Arc::new(ChannelNotify::new(bbt_sender)))).await;
    bbt_client.start_notify(ListenerId::default(), Scope::NewBlockTemplate(NewBlockTemplateScope {})).await.unwrap();
    let submitted_block_total_txs = Arc::new(AtomicUsize::new(0));
    let (subscription_task_shutdown_sender, mut subscription_task_shutdown_receiver) = oneshot::channel();

    let submit_block_pool = client_manager.new_client_pool(SUBMIT_BLOCK_CLIENTS, 100).await;
    let submit_block_pool_tasks = submit_block_pool.start(|c, block: RpcBlock| async move {
        let _sw = kaspa_core::time::Stopwatch::<500>::with_threshold("sb");
        loop {
            match c.submit_block(block.clone(), false).await {
                Ok(response) => {
                    assert_eq!(response.report, kaspa_rpc_core::SubmitBlockReport::Success);
                    break;
                }
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
        false
    });

    let cc = bbt_client.clone();
    let exec = executing.clone();
    let notification_rx = bbt_receiver.clone();
    let pac = pay_address.clone();
    let miner_receiver_task = tokio::spawn(async move {
        while let Ok(notification) = notification_rx.recv().await {
            match notification {
                Notification::NewBlockTemplate(_) => {
                    while notification_rx.try_recv().is_ok() {
                        // Drain the channel
                    }
                    // let _sw = kaspa_core::time::Stopwatch::<500>::with_threshold("bbt");
                    *current_template.lock() = cc.get_block_template(pac.clone(), vec![]).await.unwrap();
                }
                _ => panic!(),
            }
            if !exec.load(Ordering::Relaxed) {
                kaspa_core::warn!("Test is over, stopping miner receiver loop");
                break;
            }
        }
        kaspa_core::warn!("Miner receiver task exited");
    });

    let block_sender = submit_block_pool.sender();
    let exec = executing.clone();
    let cc = Arc::new(bbt_client.clone());
    let txs_counter = submitted_block_total_txs.clone();
    let miner_loop_task = tokio::spawn(async move {
        for i in 0..BLOCK_COUNT {
            // Simulate mining time
            let timeout = max((dist.sample(&mut thread_rng()) * 1000.0) as u64, 1);
            tokio::time::sleep(Duration::from_millis(timeout)).await;

            // Read the most up-to-date block template
            let mut block = current_template_consume.lock().block.clone();
            // Use index as nonce to avoid duplicate blocks
            block.header.nonce = i as u64;

            let ctc = current_template_consume.clone();
            let ccc = cc.clone();
            let pac = pay_address.clone();
            tokio::spawn(async move {
                // let _sw = kaspa_core::time::Stopwatch::<500>::with_threshold("bbt");
                // We used the current template so let's refetch a new template with new txs
                *ctc.lock() = ccc.get_block_template(pac, vec![]).await.unwrap();
            });

            let bs = block_sender.clone();
            txs_counter.fetch_add(block.transactions.len() - 1, Ordering::SeqCst);
            tokio::spawn(async move {
                // Simulate communication delay. TODO: consider adding gaussian noise
                tokio::time::sleep(Duration::from_millis(comm_delay)).await;
                let _ = bs.send(block).await;
            });
            if !exec.load(Ordering::Relaxed) {
                kaspa_core::warn!("Test is over, stopping miner loop");
                break;
            }
        }
        if exec.swap(false, Ordering::Relaxed) {
            kaspa_core::warn!("Miner loop task triggered the shutdown");
        }
        bbt_client.stop_notify(ListenerId::default(), Scope::NewBlockTemplate(NewBlockTemplateScope {})).await.unwrap();
        bbt_client.disconnect().await.unwrap();
        block_sender.close();
        subscription_task_shutdown_sender.send(()).unwrap();
        kaspa_core::warn!("Miner loop task exited");
    });

    let submit_tx_pool = client_manager.new_client_pool::<(usize, Arc<Transaction>)>(SUBMIT_TX_CLIENTS, 100).await;
    let submit_tx_pool_tasks = submit_tx_pool.start(|c, (i, tx)| async move {
        match c.submit_transaction(tx.as_ref().into(), false).await {
            Ok(_) => {}
            Err(RpcError::General(msg)) if msg.contains("orphan") => {
                kaspa_core::error!("\n\n\n{msg}\n\n");
                kaspa_core::error!("Submitted {} transactions, exiting tx submit loop", i);
                return true;
            }
            Err(e) => panic!("{e}"),
        }
        false
    });

    let tx_sender = submit_tx_pool.sender();
    let exec = executing.clone();
    let cc = client.clone();
    //let mut tps_pressure = if MEMPOOL_TARGET < u64::MAX { u64::MAX } else { TPS_PRESSURE };
    let mut tps_pressure = TPS_PRESSURE;
    let mut last_log_time = Instant::now() - Duration::from_secs(5);
    let mut log_index = 0;
    let tx_sender_task = tokio::spawn(async move {
        for (i, tx) in txs.into_iter().enumerate() {
            if tps_pressure != u64::MAX {
                tokio::time::sleep(std::time::Duration::from_secs_f64(1.0 / tps_pressure as f64)).await;
            }
            if last_log_time.elapsed() > Duration::from_millis(100) {
                let mut mempool_size = cc.get_info().await.unwrap().mempool_size;
                if log_index % 10 == 0 {
                    info!("Mempool size: {:#?}, txs submitted: {}", mempool_size, i);
                }
                log_index += 1;
                last_log_time = Instant::now();

                if mempool_size > (MEMPOOL_TARGET as f32 * 1.05) as u64 {
                    tps_pressure = TPS_PRESSURE;
                    while mempool_size > MEMPOOL_TARGET {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        mempool_size = cc.get_info().await.unwrap().mempool_size;
                        if log_index % 10 == 0 {
                            info!("Mempool size: {:#?} (targeting {:#?}), txs submitted: {}", mempool_size, MEMPOOL_TARGET, i);
                        }
                        log_index += 1;
                    }
                }
            }
            match tx_sender.send((i, tx)).await {
                Ok(_) => {}
                Err(err) => {
                    kaspa_core::error!("Tx sender channel returned error {err}");
                    break;
                }
            }
            if !exec.load(Ordering::Relaxed) {
                break;
            }
        }

        kaspa_core::warn!("Tx sender task, waiting for mempool to drain..");
        loop {
            if !exec.load(Ordering::Relaxed) {
                break;
            }
            let mempool_size = cc.get_info().await.unwrap().mempool_size;
            info!("Mempool size: {:#?}", mempool_size);
            if mempool_size == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        if exec.swap(false, Ordering::Relaxed) {
            kaspa_core::warn!("Tx sender task triggered the shutdown");
        }
        kaspa_core::warn!("Tx sender task exited");
    });

    const SUBSCRIBE_WORKERS: usize = 25;
    let subscriber_pool = Arc::new(SubscriberPool::new(SUBSCRIBE_WORKERS, NOTIFY_CLIENTS, &params));
    let subscribing_clients = create_subscribing_clients(&client_manager).await;
    let exec = executing.clone();
    let nc = subscribing_clients.clone();
    let notification_drainer_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            nc.iter().for_each(|x| while let Ok(_) = x.client.notification_channel_receiver().try_recv() {});
            if !exec.load(Ordering::Relaxed) {
                kaspa_core::warn!("Test is over, stopping notification drainer loop");
                break;
            }
        }
        kaspa_core::warn!("Notification drainer task exited");
    });

    let subscriber_pool_sender = subscriber_pool.sender();
    let pool = subscriber_pool.clone();
    let subscription_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        warn!("Starting basic subscriptions...");
        for client in subscribing_clients.iter().cloned() {
            subscriber_pool_sender.send(SubscribeCommand::StartBasic(client)).await.unwrap();
        }
        pool.wait_for_feedback(NOTIFY_CLIENTS).await;
        warn!("Basic subscriptions started");

        let mut cycle: usize = 0;
        let mut stopwatch = std::time::Instant::now();
        loop {
            if cycle == 0 {
                tokio::select! {
                    biased;
                    _ = &mut subscription_task_shutdown_receiver => {
                        kaspa_core::warn!("Test is over, stopping subscription loop");
                        break;
                    }
                    _ = tokio::time::sleep(
                        stopwatch + std::time::Duration::from_secs(5) - std::time::Instant::now(),
                    ) => {}
                }
                stopwatch = std::time::Instant::now();
            }
            cycle += 1;

            if active_utxos_changed_subscriptions && cycle <= max_cycles {
                warn!("Cycle {cycle} - Starting UTXOs notifications...");
                for client in subscribing_clients.iter().cloned() {
                    subscriber_pool_sender.send(SubscribeCommand::StartUtxosChanged(client)).await.unwrap();
                }
                pool.wait_for_feedback(NOTIFY_CLIENTS).await;
                warn!("Cycle {cycle} - UTXOs notifications started");
            }

            tokio::select! {
                biased;
                _ = &mut subscription_task_shutdown_receiver => {
                    kaspa_core::warn!("Test is over, stopping subscription loop");
                    break;
                }
                _ = tokio::time::sleep(
                    stopwatch + std::time::Duration::from_secs(utxos_changed_cycle_seconds - (utxos_changed_cycle_seconds / 3))
                        - std::time::Instant::now(),
                ) => {}
            }
            stopwatch = std::time::Instant::now();

            if active_utxos_changed_subscriptions && cycle < max_cycles {
                warn!("Cycle {cycle} - Stopping UTXOs notifications...");
                for client in subscribing_clients.iter().cloned() {
                    subscriber_pool_sender.send(SubscribeCommand::StopUtxosChanged(client)).await.unwrap();
                }
                pool.wait_for_feedback(NOTIFY_CLIENTS).await;
                warn!("Cycle {cycle} - UTXOs notifications stopped");
            }

            tokio::select! {
                biased;
                _ = &mut subscription_task_shutdown_receiver => {
                    kaspa_core::warn!("Test is over, stopping subscription loop");
                    break;
                }
                _ = tokio::time::sleep(
                    stopwatch + std::time::Duration::from_secs(utxos_changed_cycle_seconds / 3) - std::time::Instant::now(),
                ) => {}
            }
            stopwatch = std::time::Instant::now();
        }
        for subscribing_client in subscribing_clients.iter().cloned() {
            subscribing_client.client.disconnect().await.unwrap();
        }
        kaspa_core::warn!("Subscription loop task exited");
    });

    let (stat_task_shutdown_sender, stat_task_shutdown_receiver) = oneshot::channel();
    let stat_recorder_task = stat_recorder_task(STAT_FOLDER.to_owned(), None, stat_task_shutdown_receiver);

    let _ = join!(miner_receiver_task, miner_loop_task, subscription_task, tx_sender_task, notification_drainer_task);

    kaspa_core::warn!("Closing submit block and tx pools");
    submit_block_pool.close();
    submit_tx_pool.close();
    subscriber_pool.close();

    kaspa_core::warn!("Waiting for submit block pool to exit...");
    join_all(submit_block_pool_tasks).await;
    kaspa_core::warn!("Submit block pool exited");

    kaspa_core::warn!("Waiting for submit tx pool to exit...");
    join_all(submit_tx_pool_tasks).await;
    kaspa_core::warn!("Submit tx pool exited");

    kaspa_core::warn!("Waiting for subscriber pool to exit...");
    join_all(subscriber_pool.join_handles().unwrap()).await;
    kaspa_core::warn!("Subscriber pool exited");

    kaspa_core::warn!("Waiting for memory monitor and stat recorder to exit...");
    tick_service.shutdown();
    stat_task_shutdown_sender.send(()).unwrap();
    let _ = join!(memory_monitor_task, stat_recorder_task);

    //
    // Fold-up
    //
    kaspa_core::info!("Signal the server to shutdown");
    client.shutdown().await.unwrap();
    kaspa_core::warn!("Disconnect main client");
    client.disconnect().await.unwrap();
    drop(client);

    kaspa_core::warn!("Waiting for the server to exit...");
    server_process.wait().await.expect("failed to wait for the server process");
}

async fn _utxos_changed_subscriptions_client(cycle_seconds: Option<u64>, max_cycles: usize) {
    init_allocator_with_default_settings();
    kaspa_core::panic::configure_panic();
    kaspa_core::log::try_init_logger(
        "INFO, kaspa_core::time=debug, kaspa_rpc_core=debug, kaspa_grpc_client=debug, kaspa_notify=info, kaspa_notify::address::tracker=debug, kaspa_notify::listener=debug, kaspa_notify::subscription::single=debug, kaspa_mining::monitor=debug, kaspa_testing_integration::subscribe_benchmarks=trace", 
    );

    if TX_COUNT < TX_LEVEL_WIDTH {
        panic!()
    }

    let active_utxos_changed_subscriptions = cycle_seconds.is_some();
    let utxos_changed_cycle_seconds: u64 = cycle_seconds.unwrap_or(1200);
    assert!(utxos_changed_cycle_seconds > 20);

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

    // Start the server
    info!("Launching the server...");
    let daemon_args = DaemonArgs::new(
        args.rpclisten.map(|x| x.normalize(0).port).unwrap(),
        args.listen.map(|x| x.normalize(0).port).unwrap(),
        prealloc_sk.display_secret().to_string(),
        Some("ucs-server".to_owned()),
    );
    let server_start_time = std::time::Instant::now();
    let mut server_process = tokio::process::Command::new("cargo")
        .args(daemon_args.to_command_args("subscribe_benchmarks::bench_utxos_changed_subscriptions_daemon"))
        .spawn()
        .expect("failed to start child process");

    // Make sure that the server was given enough time to start
    let client_start_time = server_start_time + Duration::from_secs(5);
    if client_start_time > std::time::Instant::now() {
        tokio::time::sleep(client_start_time - std::time::Instant::now()).await;
    }

    let client_manager = Arc::new(ClientManager::new(&args));
    let tick_service = Arc::new(TickService::new());

    let _tick_task = TickTask::new(tick_service.clone());
    let _memory_monitor_task = MemoryMonitorTask::build(tick_service.clone(), "client", Duration::from_secs(1), MAX_MEMORY);

    let client = client_manager.new_client().await;
    let bbt_client = client_manager.new_client().await;

    // The time interval between Poisson(lambda) events distributes ~Exp(lambda)
    let dist: Exp<f64> = Exp::new(params.bps() as f64).unwrap();
    let comm_delay = 1000;

    // Mining key and address
    let (sk, pk) = &secp256k1::generate_keypair(&mut thread_rng());
    let pay_address =
        Address::new(network.network_type().into(), kaspa_addresses::Version::PubKey, &pk.x_only_public_key().0.serialize());
    debug!("Generated private key {} and address {}", sk.display_secret(), pay_address);

    let current_template = Arc::new(Mutex::new(bbt_client.get_block_template(pay_address.clone(), vec![]).await.unwrap()));
    let current_template_consume = current_template.clone();

    let executing = Arc::new(AtomicBool::new(true));
    let (bbt_sender, bbt_receiver) = async_channel::unbounded();
    bbt_client.start(Some(Arc::new(ChannelNotify::new(bbt_sender)))).await;
    bbt_client.start_notify(ListenerId::default(), Scope::NewBlockTemplate(NewBlockTemplateScope {})).await.unwrap();
    let submitted_block_total_txs = Arc::new(AtomicUsize::new(0));
    let (subscription_task_shutdown_sender, mut subscription_task_shutdown_receiver) = oneshot::channel();

    let submit_block_pool = client_manager.new_client_pool(SUBMIT_BLOCK_CLIENTS, 100).await;
    let submit_block_pool_tasks = submit_block_pool.start(|c, block: RpcBlock| async move {
        let _sw = kaspa_core::time::Stopwatch::<500>::with_threshold("sb");
        loop {
            match c.submit_block(block.clone(), false).await {
                Ok(response) => {
                    assert_eq!(response.report, kaspa_rpc_core::SubmitBlockReport::Success);
                    break;
                }
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
        false
    });

    let cc = bbt_client.clone();
    let exec = executing.clone();
    let notification_rx = bbt_receiver.clone();
    let pac = pay_address.clone();
    let miner_receiver_task = tokio::spawn(async move {
        while let Ok(notification) = notification_rx.recv().await {
            match notification {
                Notification::NewBlockTemplate(_) => {
                    while notification_rx.try_recv().is_ok() {
                        // Drain the channel
                    }
                    // let _sw = kaspa_core::time::Stopwatch::<500>::with_threshold("bbt");
                    *current_template.lock() = cc.get_block_template(pac.clone(), vec![]).await.unwrap();
                }
                _ => panic!(),
            }
            if !exec.load(Ordering::Relaxed) {
                kaspa_core::warn!("Test is over, stopping miner receiver loop");
                break;
            }
        }
        kaspa_core::warn!("Miner receiver task exited");
    });

    let block_sender = submit_block_pool.sender();
    let exec = executing.clone();
    let cc = Arc::new(bbt_client.clone());
    let txs_counter = submitted_block_total_txs.clone();
    let miner_loop_task = tokio::spawn(async move {
        for i in 0..BLOCK_COUNT {
            // Simulate mining time
            let timeout = max((dist.sample(&mut thread_rng()) * 1000.0) as u64, 1);
            tokio::time::sleep(Duration::from_millis(timeout)).await;

            // Read the most up-to-date block template
            let mut block = current_template_consume.lock().block.clone();
            // Use index as nonce to avoid duplicate blocks
            block.header.nonce = i as u64;

            let ctc = current_template_consume.clone();
            let ccc = cc.clone();
            let pac = pay_address.clone();
            tokio::spawn(async move {
                // let _sw = kaspa_core::time::Stopwatch::<500>::with_threshold("bbt");
                // We used the current template so let's refetch a new template with new txs
                *ctc.lock() = ccc.get_block_template(pac, vec![]).await.unwrap();
            });

            let bs = block_sender.clone();
            txs_counter.fetch_add(block.transactions.len() - 1, Ordering::SeqCst);
            tokio::spawn(async move {
                // Simulate communication delay. TODO: consider adding gaussian noise
                tokio::time::sleep(Duration::from_millis(comm_delay)).await;
                let _ = bs.send(block).await;
            });
            if !exec.load(Ordering::Relaxed) {
                kaspa_core::warn!("Test is over, stopping miner loop");
                break;
            }
        }
        if exec.swap(false, Ordering::Relaxed) {
            kaspa_core::warn!("Miner loop task triggered the shutdown");
        }
        bbt_client.stop_notify(ListenerId::default(), Scope::NewBlockTemplate(NewBlockTemplateScope {})).await.unwrap();
        bbt_client.disconnect().await.unwrap();
        block_sender.close();
        subscription_task_shutdown_sender.send(()).unwrap();
        kaspa_core::warn!("Miner loop task exited");
    });

    let submit_tx_pool = client_manager.new_client_pool::<(usize, Arc<Transaction>)>(SUBMIT_TX_CLIENTS, 100).await;
    let submit_tx_pool_tasks = submit_tx_pool.start(|c, (i, tx)| async move {
        match c.submit_transaction(tx.as_ref().into(), false).await {
            Ok(_) => {}
            Err(RpcError::General(msg)) if msg.contains("orphan") => {
                kaspa_core::error!("\n\n\n{msg}\n\n");
                kaspa_core::error!("Submitted {} transactions, exiting tx submit loop", i);
                return true;
            }
            Err(e) => panic!("{e}"),
        }
        false
    });

    let tx_sender = submit_tx_pool.sender();
    let exec = executing.clone();
    let cc = client.clone();
    //let mut tps_pressure = if MEMPOOL_TARGET < u64::MAX { u64::MAX } else { TPS_PRESSURE };
    let mut tps_pressure = TPS_PRESSURE;
    let mut last_log_time = Instant::now() - Duration::from_secs(5);
    let mut log_index = 0;
    let tx_sender_task = tokio::spawn(async move {
        for (i, tx) in txs.into_iter().enumerate() {
            if tps_pressure != u64::MAX {
                tokio::time::sleep(std::time::Duration::from_secs_f64(1.0 / tps_pressure as f64)).await;
            }
            if last_log_time.elapsed() > Duration::from_millis(100) {
                let mut mempool_size = cc.get_info().await.unwrap().mempool_size;
                if log_index % 10 == 0 {
                    info!("Mempool size: {:#?}, txs submitted: {}", mempool_size, i);
                }
                log_index += 1;
                last_log_time = Instant::now();

                if mempool_size > (MEMPOOL_TARGET as f32 * 1.05) as u64 {
                    tps_pressure = TPS_PRESSURE;
                    while mempool_size > MEMPOOL_TARGET {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        mempool_size = cc.get_info().await.unwrap().mempool_size;
                        if log_index % 10 == 0 {
                            info!("Mempool size: {:#?} (targeting {:#?}), txs submitted: {}", mempool_size, MEMPOOL_TARGET, i);
                        }
                        log_index += 1;
                    }
                }
            }
            match tx_sender.send((i, tx)).await {
                Ok(_) => {}
                Err(err) => {
                    kaspa_core::error!("Tx sender channel returned error {err}");
                    break;
                }
            }
            if !exec.load(Ordering::Relaxed) {
                break;
            }
        }

        kaspa_core::warn!("Tx sender task, waiting for mempool to drain..");
        loop {
            if !exec.load(Ordering::Relaxed) {
                break;
            }
            let mempool_size = cc.get_info().await.unwrap().mempool_size;
            info!("Mempool size: {:#?}", mempool_size);
            if mempool_size == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        if exec.swap(false, Ordering::Relaxed) {
            kaspa_core::warn!("Tx sender task triggered the shutdown");
        }
        kaspa_core::warn!("Tx sender task exited");
    });

    const SUBSCRIBE_WORKERS: usize = 25;
    let subscriber_pool = Arc::new(SubscriberPool::new(SUBSCRIBE_WORKERS, NOTIFY_CLIENTS, &params));
    let subscribing_clients = create_subscribing_clients(&client_manager).await;
    let exec = executing.clone();
    let nc = subscribing_clients.clone();
    let notification_drainer_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            nc.iter().for_each(|x| while let Ok(_) = x.client.notification_channel_receiver().try_recv() {});
            if !exec.load(Ordering::Relaxed) {
                kaspa_core::warn!("Test is over, stopping notification drainer loop");
                break;
            }
        }
        kaspa_core::warn!("Notification drainer task exited");
    });

    let subscriber_pool_sender = subscriber_pool.sender();
    let pool = subscriber_pool.clone();
    let subscription_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        warn!("Starting basic subscriptions...");
        for client in subscribing_clients.iter().cloned() {
            subscriber_pool_sender.send(SubscribeCommand::StartBasic(client)).await.unwrap();
        }
        pool.wait_for_feedback(NOTIFY_CLIENTS).await;
        warn!("Basic subscriptions started");

        let mut cycle: usize = 0;
        let mut stopwatch = std::time::Instant::now();
        loop {
            if cycle == 0 {
                tokio::select! {
                    biased;
                    _ = &mut subscription_task_shutdown_receiver => {
                        kaspa_core::warn!("Test is over, stopping subscription loop");
                        break;
                    }
                    _ = tokio::time::sleep(
                        stopwatch + std::time::Duration::from_secs(5) - std::time::Instant::now(),
                    ) => {}
                }
                stopwatch = std::time::Instant::now();
            }
            cycle += 1;

            if active_utxos_changed_subscriptions && cycle <= max_cycles {
                warn!("Cycle {cycle} - Starting UTXOs notifications...");
                for client in subscribing_clients.iter().cloned() {
                    subscriber_pool_sender.send(SubscribeCommand::StartUtxosChanged(client)).await.unwrap();
                }
                pool.wait_for_feedback(NOTIFY_CLIENTS).await;
                warn!("Cycle {cycle} - UTXOs notifications started");
            }

            tokio::select! {
                biased;
                _ = &mut subscription_task_shutdown_receiver => {
                    kaspa_core::warn!("Test is over, stopping subscription loop");
                    break;
                }
                _ = tokio::time::sleep(
                    stopwatch + std::time::Duration::from_secs(utxos_changed_cycle_seconds - (utxos_changed_cycle_seconds / 3))
                        - std::time::Instant::now(),
                ) => {}
            }
            stopwatch = std::time::Instant::now();

            if active_utxos_changed_subscriptions && cycle < max_cycles {
                warn!("Cycle {cycle} - Stopping UTXOs notifications...");
                for client in subscribing_clients.iter().cloned() {
                    subscriber_pool_sender.send(SubscribeCommand::StopUtxosChanged(client)).await.unwrap();
                }
                pool.wait_for_feedback(NOTIFY_CLIENTS).await;
                warn!("Cycle {cycle} - UTXOs notifications stopped");
            }

            tokio::select! {
                biased;
                _ = &mut subscription_task_shutdown_receiver => {
                    kaspa_core::warn!("Test is over, stopping subscription loop");
                    break;
                }
                _ = tokio::time::sleep(
                    stopwatch + std::time::Duration::from_secs(utxos_changed_cycle_seconds / 3) - std::time::Instant::now(),
                ) => {}
            }
            stopwatch = std::time::Instant::now();
        }
        for subscribing_client in subscribing_clients.iter().cloned() {
            subscribing_client.client.disconnect().await.unwrap();
        }
        kaspa_core::warn!("Subscription loop task exited");
    });

    let (stat_task_shutdown_sender, stat_task_shutdown_receiver) = oneshot::channel();
    let stat_recorder_task = stat_recorder_task(STAT_FOLDER.to_owned(), None, stat_task_shutdown_receiver);

    let _ = join!(miner_receiver_task, miner_loop_task, subscription_task, tx_sender_task, notification_drainer_task);

    kaspa_core::warn!("Closing submit block and tx pools");
    submit_block_pool.close();
    submit_tx_pool.close();
    subscriber_pool.close();

    kaspa_core::warn!("Waiting for submit block pool to exit...");
    join_all(submit_block_pool_tasks).await;
    kaspa_core::warn!("Submit block pool exited");

    kaspa_core::warn!("Waiting for submit tx pool to exit...");
    join_all(submit_tx_pool_tasks).await;
    kaspa_core::warn!("Submit tx pool exited");

    kaspa_core::warn!("Waiting for subscriber pool to exit...");
    join_all(subscriber_pool.join_handles().unwrap()).await;
    kaspa_core::warn!("Subscriber pool exited");

    kaspa_core::warn!("Waiting for memory monitor and stat recorder to exit...");
    tick_service.shutdown();
    stat_task_shutdown_sender.send(()).unwrap();
    let _ = join!(stat_recorder_task);

    //
    // Fold-up
    //
    kaspa_core::info!("Signal the server to shutdown");
    client.shutdown().await.unwrap();
    kaspa_core::warn!("Disconnect main client");
    client.disconnect().await.unwrap();
    drop(client);

    kaspa_core::warn!("Waiting for the server to exit...");
    server_process.wait().await.expect("failed to wait for the server process");
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_a --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_a() {
    // No subscriptions
    utxos_changed_subscriptions_client(None, 1).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_b --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_b() {
    // Single initial subscriptions, no cycles
    utxos_changed_subscriptions_client(Some(60), 1).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_c --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_c() {
    // 1 hour subscription cycles
    utxos_changed_subscriptions_client(Some(3600), usize::MAX).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_c --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_d() {
    // 20 minutes subscription cycles
    utxos_changed_subscriptions_client(Some(1200), usize::MAX).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc -- subscribe_benchmarks::bench_utxos_changed_subscriptions_footprint_d --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_e() {
    // 3 minutes subscription cycles
    utxos_changed_subscriptions_client(Some(180), usize::MAX).await;
}
