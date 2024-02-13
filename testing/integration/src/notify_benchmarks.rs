use crate::common::{
    self, client_notify::ChannelNotify, daemon::Daemon, tasks::memory_monitor::MemoryMonitorTask, utils::CONTRACT_FACTOR,
};
use futures_util::future::join_all;
use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::params::Params;
use kaspa_consensus_core::{constants::SOMPI_PER_KASPA, network::NetworkType, tx::Transaction};
use kaspa_core::{debug, info, task::tick::TickService, warn};
use kaspa_math::Uint256;
use kaspa_notify::{
    listener::ListenerId,
    scope::{NewBlockTemplateScope, Scope, UtxosChangedScope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::{api::rpc::RpcApi, Notification, RpcBlock, RpcError};
use kaspa_txscript::pay_to_address_script;
use kaspa_utils::fd_budget;
use kaspad_lib::args::Args;
use parking_lot::Mutex;
use rand::thread_rng;
use rand_distr::{Distribution, Exp};
use std::{
    cmp::max,
    io::Write,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{join, sync::oneshot, time::sleep};
use workflow_perf_monitor::mem::{get_process_memory_info, ProcessMemoryInfo};

/// ## Initial research
///
/// Some configurations were tested with
///
/// NOTIFY_CLIENTS = 500
/// MAX_ADDRESSES = 1_000_000
/// WALLET_ADDRESSES = 800
///
/// Address filtering at server level only (gRPC, wRPC server)
///
/// Basic subscriptions to:
///
/// - BlockAdded
/// - VirtualDaaScoreChanged
/// - VirtualChainChangedScope { include_accepted_transaction_ids: i % 2 == 0 }
///
/// and considering `cycle_seconds` & `kaspa_notify::address::tracker::Indexes as
///
/// A) No UtxosChanged subscriptions:   None        IndexVec                    
/// B) Sorted vector subscriptions:     Some(30)    IndexVec
/// C) HashSet subscriptions:           Some(30)    IndexSet
///
/// After 4 hours runs, following approximate memory consumptions were measured:
///
/// - A => 10.5 GB
/// - B => 12.7 GB
/// - C => 13.8 GB
///
/// Conclusions:
///
/// - B consumes ~2.2 GB
/// - C consumes ~3.3 GB, an increase of 50% over B
///
/// In current simple implementation, sorted vectors exhibit a worst case performance of O(N^2) for insertion
/// and removal and O(log N) for lookups.
///
/// HashSets and HashMaps are consequently favored over sorted vectors, being a good tradeoff between speed and size.
///
/// ## Further work
///
/// Following changes added:
///
/// - Single UtxosChangedSubscription with inner mutability, preventing reallocation
/// - The address vector of a mutation submitted to a UtxosChangedSubscription is retained from the server (gRPC or wRPC) all the way
///   up to the index processor without ever reallocating
/// - Unique `UtxosChangedSubscription` with state `UtxosChangedSubscriptionState::All` instance shared to every broadcaster in order
///   to group subscriptions of this state under the same object
///
/// Basic subscriptions are restricted to VirtualDaaScoreChanged only, strongly reducing the server to clients notification traffic.
///
/// Address filtering:
///
/// - server (gRPC & wRPC servers)
/// - rpc-core
/// - index processor
///
/// A memory footprint measurement is computed as the average of the samples collected every 5 seconds between minute 65 and the end of the benchmark.
async fn bench_utxos_changed_subscriptions_footprint(cycle_seconds: Option<u64>, max_cycles: usize) {
    init_allocator_with_default_settings();
    kaspa_core::panic::configure_panic();
    kaspa_core::log::try_init_logger(
        "INFO, kaspa_core::time=debug, kaspa_rpc_core=debug, kaspa_grpc_client=debug, kaspa_notify=info, kaspa_notify::address::tracker=debug, kaspa_notify::listener=debug, kaspa_notify::subscription::single=debug, kaspa_mining::monitor=debug, kaspa_testing_integration::notify_benchmarks=trace", 
    );

    // Constants
    const BLOCK_COUNT: usize = usize::MAX;

    const MEMPOOL_TARGET: u64 = 10_000;
    const TX_COUNT: usize = 1_400_000;
    const TX_LEVEL_WIDTH: usize = 20_000;
    const TPS_PRESSURE: u64 = 100;

    const SUBMIT_BLOCK_CLIENTS: usize = 20;
    const SUBMIT_TX_CLIENTS: usize = 2;

    if TX_COUNT < TX_LEVEL_WIDTH {
        panic!()
    }

    #[cfg(feature = "heap")]
    const MAX_MEMORY: u64 = 22_000_000_000;
    #[cfg(not(feature = "heap"))]
    const MAX_MEMORY: u64 = 31_000_000_000;

    const NOTIFY_CLIENTS: usize = 500;
    const MAX_ADDRESSES: usize = 1_000_000;
    const WALLET_ADDRESSES: usize = 800;

    const STAT_FOLDER: &'static str = "../../../analyze/mem-logs";

    let active_utxos_changed_subscriptions = cycle_seconds.is_some();
    let utxos_changed_cycle_seconds: u64 = cycle_seconds.unwrap_or(1200);
    assert!(utxos_changed_cycle_seconds > 20);

    let tick_service = Arc::new(TickService::new());
    let memory_monitor = MemoryMonitorTask::new(tick_service.clone(), "combined", Duration::from_secs(1), MAX_MEMORY);
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

    let args = Args {
        simnet: true,
        disable_upnp: true, // UPnP registration might take some time and is not needed for this test
        enable_unsynced_mining: true,
        num_prealloc_utxos: Some(TX_LEVEL_WIDTH as u64 * CONTRACT_FACTOR),
        prealloc_address: Some(prealloc_address.to_string()),
        prealloc_amount: 500 * SOMPI_PER_KASPA,
        block_template_cache_lifetime: Some(0),
        rpc_max_clients: 2500,
        ..Default::default()
    };
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

    let fd_total_budget = fd_budget::limit();
    let mut daemon = Daemon::new_random_with_args(args, fd_total_budget);
    let client = daemon.start().await;
    let bbt_client = daemon.new_client().await;

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
    let (sender, receiver) = async_channel::unbounded();
    bbt_client.start(Some(Arc::new(ChannelNotify::new(sender)))).await;
    bbt_client.start_notify(ListenerId::default(), Scope::NewBlockTemplate(NewBlockTemplateScope {})).await.unwrap();
    let submitted_block_total_txs = Arc::new(AtomicUsize::new(0));
    let (subscription_task_shutdown_sender, mut subscription_task_shutdown_receiver) = oneshot::channel();
    let (stat_task_shutdown_sender, mut stat_task_shutdown_receiver) = oneshot::channel();

    let submit_block_pool = daemon
        .new_client_pool(SUBMIT_BLOCK_CLIENTS, 100, |c, block: RpcBlock| async move {
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
        })
        .await;

    let submit_tx_pool = daemon
        .new_client_pool::<(usize, Arc<Transaction>), _, _>(SUBMIT_TX_CLIENTS, 100, |c, (i, tx)| async move {
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
        })
        .await;

    let cc = bbt_client.clone();
    let exec = executing.clone();
    let notification_rx = receiver.clone();
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
        kaspa_core::warn!("Miner receiver loop task exited");
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
        subscription_task_shutdown_sender.send(()).unwrap();
        kaspa_core::warn!("Miner loop task exited");
    });

    let tx_sender = submit_tx_pool.sender();
    let exec = executing.clone();
    let cc = client.clone();
    let mut tps_pressure = if MEMPOOL_TARGET < u64::MAX { u64::MAX } else { TPS_PRESSURE };
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

    let notify_clients = daemon.new_clients(NOTIFY_CLIENTS).await;
    let network_id = daemon.network;

    let notification_receive_tasks = notify_clients
        .iter()
        .cloned()
        .enumerate()
        .map(|(_, client)| {
            let exec = executing.clone();
            tokio::spawn(async move {
                let mut counter: u64 = 0;
                loop {
                    match tokio::time::timeout(std::time::Duration::from_secs(3), client.notification_channel_receiver().recv()).await
                    {
                        Ok(Ok(_)) => {
                            counter += 1;
                            if counter % 1_000 == 0 {
                                // trace!("Client #{} received {} notifications", i, counter);
                            }
                        }
                        Ok(Err(_)) => {}
                        Err(_) => {}
                    }
                    if !exec.load(Ordering::Relaxed) {
                        // kaspa_core::warn!("Test is over, stopping notification receive loop for client #{}", i);
                        break;
                    }
                }
            })
        })
        .collect_vec();

    let subscription_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        warn!("Starting basic subscriptions...");
        join_all(notify_clients.iter().cloned().enumerate().map(|(_, client)| {
            tokio::spawn(async move {
                // client.start_notify(0, BlockAddedScope {}.into()).await.unwrap();
                client.start_notify(0, VirtualDaaScoreChangedScope {}.into()).await.unwrap();
                // client
                //     .start_notify(0, VirtualChainChangedScope { include_accepted_transaction_ids: i % 2 == 0 }.into())
                //     .await
                //     .unwrap();
            })
        }))
        .await;
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
                        stopwatch + std::time::Duration::from_secs(utxos_changed_cycle_seconds) - std::time::Instant::now(),
                    ) => {}
                }
                stopwatch = std::time::Instant::now();
            }
            cycle += 1;

            if active_utxos_changed_subscriptions && cycle <= max_cycles {
                warn!("Cycle #{cycle} - Starting UTXOs notifications...");
                join_all(notify_clients.iter().cloned().enumerate().map(|(i, client)| {
                    tokio::spawn(async move {
                        loop {
                            // Process in heaviest to lightest requests order, maximizing messages memory footprint
                            // between notifiers and from notifier to broadcasters at grpc server and rpc core levels
                            let max_address = ((NOTIFY_CLIENTS - i) * MAX_ADDRESSES / NOTIFY_CLIENTS) + 1;
                            let min_address = if (NOTIFY_CLIENTS - i) % (NOTIFY_CLIENTS / 5) == 0 {
                                // Create a typical UTXOs monitoring service subscription scope
                                0
                            } else {
                                // Create a typical wallet subscription scope
                                max_address.max(WALLET_ADDRESSES) - WALLET_ADDRESSES
                            };
                            let addresses = (min_address..max_address)
                                .map(|x| {
                                    Address::new(
                                        network_id.into(),
                                        kaspa_addresses::Version::PubKey,
                                        &Uint256::from_u64(x as u64).to_le_bytes(),
                                    )
                                })
                                .collect_vec();
                            match client.start_notify(0, UtxosChangedScope::new(addresses).into()).await {
                                Ok(_) => {
                                    break;
                                }
                                Err(_) => {
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                            }
                        }
                    })
                }))
                .await;
                warn!("Cycle #{cycle} - UTXOs notifications started");
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

            if active_utxos_changed_subscriptions && cycle <= max_cycles {
                warn!("Cycle #{cycle} - Stopping UTXOs notifications...");
                join_all(notify_clients.iter().cloned().map(|client| {
                    tokio::spawn(async move {
                        loop {
                            match client.stop_notify(0, UtxosChangedScope::new(vec![]).into()).await {
                                Ok(_) => {
                                    break;
                                }
                                Err(_) => {
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                            }
                        }
                    })
                }))
                .await;
                warn!("Cycle #{cycle} - UTXOs notifications stopped");
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
        for client in notify_clients.iter().cloned() {
            client.disconnect().await.unwrap();
        }
        kaspa_core::warn!("Subscription loop task exited");
    });

    let txs_counter = submitted_block_total_txs.clone();
    let stat_recorder_task = tokio::spawn(async move {
        std::fs::create_dir_all(STAT_FOLDER).unwrap();
        {
            let f = std::fs::File::create(format!("{}/ucs-{}.csv", STAT_FOLDER, chrono::Local::now().format("%Y-%m-%d %H-%M-%S")))
                .unwrap();
            let mut f = std::io::BufWriter::new(f);
            let start_time = std::time::Instant::now();
            let mut stopwatch = start_time;
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stat_task_shutdown_receiver => {
                        kaspa_core::warn!("Test is over, stopping subscription loop");
                        break;
                    }
                    _ = tokio::time::sleep(stopwatch + std::time::Duration::from_secs(5) - std::time::Instant::now()) => {}
                }
                stopwatch = std::time::Instant::now();
                let ProcessMemoryInfo { resident_set_size, .. } = get_process_memory_info().unwrap();
                let txs = txs_counter.load(Ordering::SeqCst);
                writeln!(f, "{}, {}, {}", (stopwatch - start_time).as_millis() as f64 / 1000.0 / 60.0, txs, resident_set_size)
                    .unwrap();
                f.flush().unwrap();
            }
        }
        kaspa_core::warn!("Stat recorder task exited");
    });

    let _ = join!(miner_receiver_task, miner_loop_task, subscription_task, tx_sender_task);

    kaspa_core::warn!("Waiting for notification receive tasks to exit...");
    join_all(notification_receive_tasks).await;
    kaspa_core::warn!("Notification receive tasks exited");

    kaspa_core::warn!("Closing submit block and tx pools");
    submit_block_pool.close();
    submit_tx_pool.close();

    kaspa_core::warn!("Waiting for submit block pool to exit...");
    submit_block_pool.join().await;
    kaspa_core::warn!("Submit block pool exited");

    kaspa_core::warn!("Waiting for submit tx pool to exit...");
    submit_tx_pool.join().await;
    kaspa_core::warn!("Submit tx pool exited");

    kaspa_core::warn!("Waiting for memory monitor and stat recorder to exit...");
    tick_service.shutdown();
    stat_task_shutdown_sender.send(()).unwrap();
    let _ = join!(memory_monitor_task, stat_recorder_task);

    //
    // Fold-up
    //
    kaspa_core::warn!("Disconnect main client");
    client.disconnect().await.unwrap();
    drop(client);

    sleep(Duration::from_millis(500)).await;
    daemon.shutdown();
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc --profile release -- notify_benchmarks::bench_utxos_changed_subscriptions_footprint_a --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_a() {
    bench_utxos_changed_subscriptions_footprint(None, 1).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc --profile release -- notify_benchmarks::bench_utxos_changed_subscriptions_footprint_b --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_b() {
    bench_utxos_changed_subscriptions_footprint(Some(1200), 10_000).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc --profile release -- notify_benchmarks::bench_utxos_changed_subscriptions_footprint_c --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_c() {
    bench_utxos_changed_subscriptions_footprint(Some(60), 10_000).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc --profile release -- notify_benchmarks::bench_utxos_changed_subscriptions_footprint_d --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_d() {
    bench_utxos_changed_subscriptions_footprint(Some(30), 10_000).await;
}

/// `cargo test --package kaspa-testing-integration --lib --features devnet-prealloc --profile release -- notify_benchmarks::bench_utxos_changed_subscriptions_footprint_e --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_utxos_changed_subscriptions_footprint_e() {
    bench_utxos_changed_subscriptions_footprint(Some(1200), 1).await;
}
