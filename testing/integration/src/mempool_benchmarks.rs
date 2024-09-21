use crate::{
    common::{
        self,
        args::ArgsBuilder,
        client_notify::ChannelNotify,
        daemon::{ClientManager, Daemon},
        utils::CONTRACT_FACTOR,
    },
    tasks::{block::group::MinerGroupTask, daemon::DaemonTask, tx::group::TxSenderGroupTask, Stopper, TasksRunner},
};
use futures_util::future::join_all;
use kaspa_addresses::Address;
use kaspa_consensus::params::Params;
use kaspa_consensus_core::{constants::SOMPI_PER_KASPA, network::NetworkType, tx::Transaction};
use kaspa_core::{debug, info};
use kaspa_notify::{
    listener::ListenerId,
    scope::{NewBlockTemplateScope, Scope},
};
use kaspa_rpc_core::{api::rpc::RpcApi, Notification, RpcError};
use kaspa_txscript::pay_to_address_script;
use kaspa_utils::fd_budget;
use kaspad_lib::args::Args;
use parking_lot::Mutex;
use rand::thread_rng;
use rand_distr::{Distribution, Exp};
use std::{
    cmp::max,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::join;

/// Run this benchmark with the following command line:
/// `cargo test --release --package kaspa-testing-integration --lib --features devnet-prealloc -- mempool_benchmarks::bench_bbt_latency --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_bbt_latency() {
    kaspa_core::log::try_init_logger("info,kaspa_core::time=debug,kaspa_mining::monitor=debug");
    // As we log the panic, we want to set it up after the logger
    kaspa_core::panic::configure_panic();

    // Constants
    const BLOCK_COUNT: usize = usize::MAX;

    const MEMPOOL_TARGET: u64 = 600_000;
    const TX_COUNT: usize = 1_400_000;
    const TX_LEVEL_WIDTH: usize = 20_000;
    const TPS_PRESSURE: u64 = u64::MAX;

    const SUBMIT_BLOCK_CLIENTS: usize = 20;
    const SUBMIT_TX_CLIENTS: usize = 2;

    if TX_COUNT < TX_LEVEL_WIDTH {
        panic!()
    }

    /*
    Logic:
       1. Use the new feature for preallocating utxos
       2. Set up a dataset with a DAG of signed txs over the preallocated utxoset
       3. Create constant mempool pressure by submitting txs (via rpc for now)
       4. Mine to the node (simulated)
       5. Measure bbt latency, real-time bps, real-time throughput, mempool draining rate (tbd)

    TODO:
        1. More measurements with statistical aggregation
        2. Save TX DAG dataset in a file for benchmark replication and stability
        3. Add P2P TX traffic by implementing a custom P2P peer which only broadcasts txs
    */

    //
    // Setup
    //
    let (prealloc_sk, prealloc_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let prealloc_address =
        Address::new(NetworkType::Simnet.into(), kaspa_addresses::Version::PubKey, &prealloc_pk.x_only_public_key().0.serialize());
    let schnorr_key = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &prealloc_sk);
    let spk = pay_to_address_script(&prealloc_address);

    let args = Args {
        simnet: true,
        disable_upnp: true, // UPnP registration might take some time and is not needed for this test
        enable_unsynced_mining: true,
        num_prealloc_utxos: Some(TX_LEVEL_WIDTH as u64 * CONTRACT_FACTOR),
        prealloc_address: Some(prealloc_address.to_string()),
        prealloc_amount: 500 * SOMPI_PER_KASPA,
        block_template_cache_lifetime: Some(0),
        ..Default::default()
    };
    let network = args.network();
    let params: Params = network.into();

    let utxoset = args.generate_prealloc_utxos(args.num_prealloc_utxos.unwrap());
    let txs = common::utils::generate_tx_dag(utxoset.clone(), schnorr_key, spk, TX_COUNT / TX_LEVEL_WIDTH, TX_LEVEL_WIDTH);
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

    let submit_block_pool = daemon.new_client_pool(SUBMIT_BLOCK_CLIENTS, 100).await;
    let submit_block_pool_tasks = submit_block_pool.start(|c, block| async move {
        let _sw = kaspa_core::time::Stopwatch::<500>::with_threshold("sb");
        let response = c.submit_block(block, false).await.unwrap();
        assert_eq!(response.report, kaspa_rpc_core::SubmitBlockReport::Success);
        false
    });

    let submit_tx_pool = daemon.new_client_pool::<(usize, Arc<Transaction>)>(SUBMIT_TX_CLIENTS, 100).await;
    let submit_tx_pool_tasks = submit_tx_pool.start(|c, (i, tx)| async move {
        match c.submit_transaction(tx.as_ref().into(), false).await {
            Ok(_) => {}
            Err(RpcError::General(msg)) if msg.contains("orphan") => {
                kaspa_core::warn!("\n\n\n{msg}\n\n");
                kaspa_core::warn!("Submitted {} transactions, exiting tx submit loop", i);
                return true;
            }
            Err(e) => panic!("{e}"),
        }
        false
    });

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
        exec.store(false, Ordering::Relaxed);
        bbt_client.stop_notify(ListenerId::default(), Scope::NewBlockTemplate(NewBlockTemplateScope {})).await.unwrap();
        bbt_client.disconnect().await.unwrap();
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
            if last_log_time.elapsed() > Duration::from_millis(200) {
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
                            info!("Mempool size: {:#?}, txs submitted: {}", mempool_size, i);
                        }
                        log_index += 1;
                    }
                }
            }
            match tx_sender.send((i, tx)).await {
                Ok(_) => {}
                Err(_) => {
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
            if mempool_size == 0 || (TX_COUNT as u64 > MEMPOOL_TARGET && mempool_size < MEMPOOL_TARGET) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        exec.store(false, Ordering::Relaxed);
        kaspa_core::warn!("Tx sender task exited");
    });

    let _ = join!(miner_receiver_task, miner_loop_task, tx_sender_task);

    submit_block_pool.close();
    submit_tx_pool.close();

    join_all(submit_block_pool_tasks).await;
    join_all(submit_tx_pool_tasks).await;

    //
    // Fold-up
    //
    client.disconnect().await.unwrap();
    drop(client);
    daemon.shutdown();
}

/// Run this benchmark with the following command line:
/// `cargo test --release --package kaspa-testing-integration --lib --features devnet-prealloc -- mempool_benchmarks::bench_bbt_latency_2 --exact --nocapture --ignored`
#[tokio::test]
#[ignore = "bmk"]
async fn bench_bbt_latency_2() {
    kaspa_core::log::try_init_logger("info,kaspa_core::time=debug,kaspa_mining::monitor=debug");
    // As we log the panic, we want to set it up after the logger
    kaspa_core::panic::configure_panic();

    // Constants
    const BLOCK_COUNT: usize = usize::MAX;

    const MEMPOOL_TARGET: u64 = 600_000;
    const TX_COUNT: usize = 1_000_000;
    const TX_LEVEL_WIDTH: usize = 300_000;
    const TPS_PRESSURE: u64 = u64::MAX;

    const SUBMIT_BLOCK_CLIENTS: usize = 20;
    const SUBMIT_TX_CLIENTS: usize = 2;

    if TX_COUNT < TX_LEVEL_WIDTH {
        panic!()
    }

    /*
    Logic:
       1. Use the new feature for preallocating utxos
       2. Set up a dataset with a DAG of signed txs over the preallocated utxoset
       3. Create constant mempool pressure by submitting txs (via rpc for now)
       4. Mine to the node (simulated)
       5. Measure bbt latency, real-time bps, real-time throughput, mempool draining rate (tbd)

    TODO:
        1. More measurements with statistical aggregation
        2. Save TX DAG dataset in a file for benchmark replication and stability
        3. Add P2P TX traffic by implementing a custom P2P peer which only broadcasts txs
    */

    //
    // Setup
    //
    let (prealloc_sk, prealloc_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let prealloc_address =
        Address::new(NetworkType::Simnet.into(), kaspa_addresses::Version::PubKey, &prealloc_pk.x_only_public_key().0.serialize());
    let schnorr_key = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &prealloc_sk);
    let spk = pay_to_address_script(&prealloc_address);

    let args = ArgsBuilder::simnet(TX_LEVEL_WIDTH as u64 * CONTRACT_FACTOR, 500)
        .prealloc_address(prealloc_address)
        .apply_args(Daemon::fill_args_with_random_ports)
        .build();

    let network = args.network();
    let params: Params = network.into();

    let utxoset = args.generate_prealloc_utxos(args.num_prealloc_utxos.unwrap());
    let txs = common::utils::generate_tx_dag(utxoset.clone(), schnorr_key, spk, TX_COUNT / TX_LEVEL_WIDTH, TX_LEVEL_WIDTH);
    common::utils::verify_tx_dag(&utxoset, &txs);
    info!("Generated overall {} txs", txs.len());

    let client_manager = Arc::new(ClientManager::new(args));
    let mut tasks = TasksRunner::new(Some(DaemonTask::build(client_manager.clone())))
        .launch()
        .await
        .task(
            MinerGroupTask::build(network, client_manager.clone(), SUBMIT_BLOCK_CLIENTS, params.bps(), BLOCK_COUNT, Stopper::Signal)
                .await,
        )
        .task(
            TxSenderGroupTask::build(
                client_manager.clone(),
                SUBMIT_TX_CLIENTS,
                false,
                txs,
                TPS_PRESSURE,
                MEMPOOL_TARGET,
                Stopper::Signal,
            )
            .await,
        );
    tasks.run().await;
    tasks.join().await;
}
