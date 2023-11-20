use crate::common::{
    client::ListeningClient,
    daemon::Daemon,
    utils::{fetch_spendable_utxos, generate_tx, mine_block, wait_for},
};
use kaspa_addresses::Address;
use kaspa_consensus::params::SIMNET_PARAMS;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{task::runtime::AsyncRuntime, trace};
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::scope::{BlockAddedScope, UtxosChangedScope, VirtualDaaScoreChangedScope};
use kaspa_rpc_core::{api::rpc::RpcApi, Notification, RpcTransactionId};
use kaspa_txscript::pay_to_address_script;
use kaspad_lib::args::Args;
use rand::thread_rng;
use std::{sync::Arc, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_sanity_test() {
    kaspa_core::log::try_init_logger("INFO");

    // let total_fd_limit =  kaspa_utils::fd_budget::get_limit() / 2 - 128;
    let total_fd_limit = 10;
    let mut kaspad1 = Daemon::new_random(total_fd_limit);
    let rpc_client1 = kaspad1.start().await;

    let mut kaspad2 = Daemon::new_random(total_fd_limit);
    let rpc_client2 = kaspad2.start().await;

    tokio::time::sleep(Duration::from_secs(1)).await;
    rpc_client1.disconnect().await.unwrap();
    drop(rpc_client1);
    kaspad1.shutdown();

    rpc_client2.disconnect().await.unwrap();
    drop(rpc_client2);
    kaspad2.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_mining_test() {
    kaspa_core::log::try_init_logger("INFO");

    let args = Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        disable_upnp: true, // UPnP registration might take some time and is not needed for this test
        ..Default::default()
    };
    // let total_fd_limit = kaspa_utils::fd_budget::get_limit() / 2 - 128;
    let total_fd_limit = 10;

    let mut kaspad1 = Daemon::new_random_with_args(args.clone(), total_fd_limit);
    let mut kaspad2 = Daemon::new_random_with_args(args, total_fd_limit);
    let rpc_client1 = kaspad1.start().await;
    let rpc_client2 = kaspad2.start().await;

    rpc_client2.add_peer(format!("127.0.0.1:{}", kaspad1.p2p_port).try_into().unwrap(), true).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await; // Let it connect
    assert_eq!(rpc_client2.get_connected_peer_info().await.unwrap().peer_info.len(), 1);

    // Mine 10 blocks to daemon #1
    let mut last_block_hash = None;
    for _ in 0..10 {
        let template = rpc_client1
            .get_block_template(Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &[0; 32]), vec![])
            .await
            .unwrap();
        last_block_hash = Some(template.block.header.hash);
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    // Expect the blocks to be relayed to daemon #2
    let dag_info = rpc_client2.get_block_dag_info().await.unwrap();
    assert_eq!(dag_info.block_count, 10);
    assert_eq!(dag_info.sink, last_block_hash.unwrap());

    // Check that acceptance data contains the expected coinbase tx ids
    let vc = rpc_client2.get_virtual_chain_from_block(kaspa_consensus::params::SIMNET_GENESIS.hash, true).await.unwrap();
    assert_eq!(vc.removed_chain_block_hashes.len(), 0);
    assert_eq!(vc.added_chain_block_hashes.len(), 10);
    assert_eq!(vc.accepted_transaction_ids.len(), 10);
    for accepted_txs_pair in vc.accepted_transaction_ids {
        assert_eq!(accepted_txs_pair.accepted_transaction_ids.len(), 1);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_utxos_propagation_test() {
    #[cfg(feature = "heap")]
    let _profiler = dhat::Profiler::builder().file_name("kaspa-testing-integration-heap.json").build();

    kaspa_core::log::try_init_logger(
        "INFO,kaspa_testing_integration=trace,kaspa_notify=debug,kaspa_rpc_core=debug,kaspa_grpc_client=debug",
    );

    let args = Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        disable_upnp: true, // UPnP registration might take some time and is not needed for this test
        utxoindex: true,
        ..Default::default()
    };
    let total_fd_limit = 10;

    let coinbase_maturity = SIMNET_PARAMS.coinbase_maturity;
    let mut kaspad1 = Daemon::new_random_with_args(args.clone(), total_fd_limit);
    let mut kaspad2 = Daemon::new_random_with_args(args, total_fd_limit);
    let rpc_client1 = kaspad1.start().await;
    let rpc_client2 = kaspad2.start().await;

    rpc_client2.add_peer(format!("127.0.0.1:{}", kaspad1.p2p_port).try_into().unwrap(), true).await.unwrap();

    let check_client = rpc_client2.clone();
    wait_for(
        50,
        20,
        move || {
            async fn peer_connected(client: GrpcClient) -> bool {
                client.get_connected_peer_info().await.unwrap().peer_info.len() == 1
            }
            Box::pin(peer_connected(check_client.clone()))
        },
        "the nodes did not connect to each other",
    )
    .await;

    // Mining key and address
    let (miner_sk, miner_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let miner_address =
        Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &miner_pk.x_only_public_key().0.serialize());
    let miner_schnorr_key = secp256k1::KeyPair::from_secret_key(secp256k1::SECP256K1, &miner_sk);
    let miner_spk = pay_to_address_script(&miner_address);

    // User key and address
    let (_user_sk, user_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let user_address =
        Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &user_pk.x_only_public_key().0.serialize());

    // Some dummy non-monitored address
    let blank_address = Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &[0; 32]);

    // Mine 1000 blocks to daemon #1
    let initial_blocks: usize = coinbase_maturity as usize;
    let mut last_block_hash = None;
    for _ in 0..initial_blocks {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        last_block_hash = Some(template.block.header.hash);
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    let check_client = rpc_client2.clone();
    wait_for(
        50,
        20,
        move || {
            async fn daa_score_reached(client: GrpcClient) -> bool {
                let virtual_daa_score = client.get_server_info().await.unwrap().virtual_daa_score;
                trace!("Virtual DAA score: {}", virtual_daa_score);
                virtual_daa_score == SIMNET_PARAMS.coinbase_maturity
            }
            Box::pin(daa_score_reached(check_client.clone()))
        },
        "the nodes did not add and relay all the initial blocks",
    )
    .await;

    // Expect the blocks to be relayed to daemon #2
    let dag_info = rpc_client2.get_block_dag_info().await.unwrap();
    assert_eq!(dag_info.block_count, initial_blocks as u64);
    assert_eq!(dag_info.sink, last_block_hash.unwrap());

    // Check that acceptance data contains the expected coinbase tx ids
    let vc = rpc_client2.get_virtual_chain_from_block(kaspa_consensus::params::SIMNET_GENESIS.hash, true).await.unwrap();
    assert_eq!(vc.removed_chain_block_hashes.len(), 0);
    assert_eq!(vc.added_chain_block_hashes.len(), initial_blocks);
    assert_eq!(vc.accepted_transaction_ids.len(), initial_blocks);
    for accepted_txs_pair in vc.accepted_transaction_ids {
        assert_eq!(accepted_txs_pair.accepted_transaction_ids.len(), 1);
    }

    // Create a multi-listener RPC client on each node...
    let mut clients = vec![ListeningClient::connect(&kaspad2).await, ListeningClient::connect(&kaspad1).await];

    // ...and subscribe each to some notifications
    for x in clients.iter_mut() {
        x.start_notify(BlockAddedScope {}.into()).await.unwrap();
        x.start_notify(UtxosChangedScope { addresses: vec![miner_address.clone(), user_address.clone()] }.into()).await.unwrap();
        x.start_notify(VirtualDaaScoreChangedScope {}.into()).await.unwrap();
    }

    // Mine some extra blocks so the latest miner reward is added to its balance and some UTXOs reach maturity
    const EXTRA_BLOCKS: usize = 10;
    for _ in 0..EXTRA_BLOCKS {
        mine_block(blank_address.clone(), &rpc_client1, &clients).await;
    }

    // Check the balance of the miner address
    let miner_balance = rpc_client2.get_balance_by_address(miner_address.clone()).await.unwrap();
    assert_eq!(miner_balance, initial_blocks as u64 * SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);
    let miner_balance = rpc_client1.get_balance_by_address(miner_address.clone()).await.unwrap();
    assert_eq!(miner_balance, initial_blocks as u64 * SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);

    // Get the miner UTXOs
    let utxos = fetch_spendable_utxos(&rpc_client1, miner_address.clone(), coinbase_maturity).await;
    assert_eq!(utxos.len(), EXTRA_BLOCKS - 1);
    for utxo in utxos.iter() {
        assert!(utxo.1.is_coinbase);
        assert_eq!(utxo.1.amount, SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);
        assert_eq!(utxo.1.script_public_key, miner_spk);
    }

    // Drain UTXOs and Virtual DAA score changed notification channels
    clients.iter().for_each(|x| x.utxos_changed_listener().unwrap().drain());
    clients.iter().for_each(|x| x.virtual_daa_score_changed_listener().unwrap().drain());

    // Spend some coins
    const NUMBER_INPUTS: usize = 2;
    const NUMBER_OUTPUTS: usize = 2;
    const TX_AMOUNT: u64 = SIMNET_PARAMS.pre_deflationary_phase_base_subsidy * (NUMBER_INPUTS as u64 * 5 - 1) / 5;
    let transaction = generate_tx(miner_schnorr_key, &utxos[0..NUMBER_INPUTS], TX_AMOUNT, NUMBER_OUTPUTS as u64, &user_address);
    rpc_client1.submit_transaction((&transaction).into(), false).await.unwrap();

    let check_client = rpc_client1.clone();
    let transaction_id = transaction.id();
    wait_for(
        50,
        20,
        move || {
            async fn transaction_in_mempool(client: GrpcClient, transaction_id: RpcTransactionId) -> bool {
                let entry = client.get_mempool_entry(transaction_id, false, false).await;
                entry.is_ok()
            }
            Box::pin(transaction_in_mempool(check_client.clone(), transaction_id))
        },
        "the transaction was not added to the mempool",
    )
    .await;

    mine_block(blank_address.clone(), &rpc_client1, &clients).await;

    // Check UTXOs changed notifications
    for x in clients.iter() {
        let Notification::UtxosChanged(uc) = x.utxos_changed_listener().unwrap().receiver.recv().await.unwrap() else {
            panic!("wrong notification type")
        };
        assert!(uc.removed.iter().any(|x| x.address.is_some() && *x.address.as_ref().unwrap() == miner_address));
        assert!(uc.added.iter().any(|x| x.address.is_some() && *x.address.as_ref().unwrap() == user_address));
        assert_eq!(uc.removed.len(), NUMBER_INPUTS);
        assert_eq!(uc.added.len(), NUMBER_OUTPUTS);
        assert_eq!(
            uc.removed.iter().map(|x| x.utxo_entry.amount).sum::<u64>(),
            SIMNET_PARAMS.pre_deflationary_phase_base_subsidy * NUMBER_INPUTS as u64
        );
        assert_eq!(uc.added.iter().map(|x| x.utxo_entry.amount).sum::<u64>(), TX_AMOUNT);
    }

    // Check the balance of both miner and user addresses
    for x in clients.iter() {
        let miner_balance = x.get_balance_by_address(miner_address.clone()).await.unwrap();
        assert_eq!(miner_balance, (initial_blocks - NUMBER_INPUTS) as u64 * SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);

        let user_balance = x.get_balance_by_address(user_address.clone()).await.unwrap();
        assert_eq!(user_balance, TX_AMOUNT);
    }

    // Terminate multi-listener clients
    for x in clients.iter() {
        x.disconnect().await.unwrap();
        x.join().await.unwrap();
    }
}

// The following test runtime parameters are required for a graceful shutdown of the gRPC server
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_cleaning_test() {
    kaspa_core::log::try_init_logger("info,kaspa_grpc_core=trace,kaspa_grpc_server=trace,kaspa_grpc_client=trace,kaspa_core=trace");
    let args = Args { devnet: true, ..Default::default() };
    let consensus_manager;
    let async_runtime;
    let core;
    {
        let total_fd_limit = 10;
        let mut kaspad1 = Daemon::new_random_with_args(args, total_fd_limit);
        let dyn_consensus_manager = kaspad1.core.find(ConsensusManager::IDENT).unwrap();
        let dyn_async_runtime = kaspad1.core.find(AsyncRuntime::IDENT).unwrap();
        consensus_manager = Arc::downgrade(&Arc::downcast::<ConsensusManager>(dyn_consensus_manager.arc_any()).unwrap());
        async_runtime = Arc::downgrade(&Arc::downcast::<AsyncRuntime>(dyn_async_runtime.arc_any()).unwrap());
        core = Arc::downgrade(&kaspad1.core);

        let rpc_client1 = kaspad1.start().await;
        rpc_client1.disconnect().await.unwrap();
        drop(rpc_client1);
        kaspad1.shutdown();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(consensus_manager.strong_count(), 0);
    assert_eq!(async_runtime.strong_count(), 0);
    assert_eq!(core.strong_count(), 0);
}
