use crate::common::{
    daemon::Daemon,
    listener::Listener,
    utils::{required_fee, wait_for},
};
use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_consensus::params::SIMNET_PARAMS;
use kaspa_consensus_core::{
    constants::TX_VERSION,
    sign::sign,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{task::runtime::AsyncRuntime, trace};
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::scope::{BlockAddedScope, Scope, UtxosChangedScope};
use kaspa_rpc_core::{api::rpc::RpcApi, BlockAddedNotification, Notification, RpcTransactionId};
use kaspa_txscript::pay_to_address_script;
use kaspad_lib::args::Args;
use rand::thread_rng;
use secp256k1::KeyPair;
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
    struct Client {
        ml_client: GrpcClient,
        block_added_listener: Listener,
        utxos_changed_listener: Listener,
    }

    impl Client {
        async fn connect(kaspad: &Daemon, miner_address: &Address, user_address: &Address) -> Self {
            let ml_client = kaspad.new_multi_listener_client().await;
            ml_client.start(None).await;
            let block_added_listener = Listener::subscribe(&ml_client, BlockAddedScope {}.into()).await;
            let utxos_changed_scope: Scope = UtxosChangedScope::new(vec![miner_address.clone(), user_address.clone()]).into();
            let utxos_changed_listener = Listener::subscribe(&ml_client, utxos_changed_scope.clone()).await;
            Client { ml_client, block_added_listener, utxos_changed_listener }
        }

        async fn disconnect(&self) -> kaspa_grpc_client::error::Result<()> {
            self.ml_client.disconnect().await
        }

        async fn join(&self) -> kaspa_grpc_client::error::Result<()> {
            self.ml_client.join().await
        }
    }

    async fn mine_block(pay_address: Address, submitting_client: &GrpcClient, listening_clients: &[Client]) {
        // Mine an extra block so the latest miner reward is added to its balance
        let template = submitting_client.get_block_template(pay_address.clone(), vec![]).await.unwrap();
        let block_hash = template.block.header.hash;
        submitting_client.submit_block(template.block, false).await.unwrap();
        for client in listening_clients.iter() {
            match client.block_added_listener.receiver.recv().await.unwrap() {
                Notification::BlockAdded(BlockAddedNotification { block }) => {
                    assert_eq!(block.header.hash, block_hash);
                }
                _ => panic!("wrong notification type"),
            }
        }
    }

    fn generate_tx(
        schnorr_key: KeyPair,
        outpoint: &TransactionOutpoint,
        utxo: &UtxoEntry,
        amount: u64,
        num_outputs: u64,
        address: &Address,
    ) -> Transaction {
        let total_in = utxo.amount;
        let total_out = total_in - required_fee(1, num_outputs);
        assert!(amount <= total_out);
        let script_public_key = pay_to_address_script(address);
        let entries = vec![utxo.clone()];
        let inputs = vec![TransactionInput { previous_outpoint: *outpoint, signature_script: vec![], sequence: 0, sig_op_count: 1 }];
        let outputs = (0..num_outputs)
            .map(|_| TransactionOutput { value: amount / num_outputs, script_public_key: script_public_key.clone() })
            .collect_vec();
        let unsigned_tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let signed_tx = sign(MutableTransaction::with_entries(unsigned_tx, entries), schnorr_key);
        signed_tx.tx
    }

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
    let initial_blocks: usize = SIMNET_PARAMS.coinbase_maturity as usize;
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

    // Create a multi-listener RPC client on each node and subscribe each to some notifications
    let clients = vec![
        Client::connect(&kaspad2, &miner_address, &user_address).await,
        Client::connect(&kaspad1, &miner_address, &user_address).await,
    ];

    // Mine some extra blocks so the latest miner reward is added to its balance
    for _ in 0..2 {
        mine_block(blank_address.clone(), &rpc_client1, &clients).await;
    }

    // Check the balance of the miner address
    let miner_balance = rpc_client2.get_balance_by_address(miner_address.clone()).await.unwrap();
    assert_eq!(miner_balance, initial_blocks as u64 * SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);
    let miner_balance = rpc_client1.get_balance_by_address(miner_address.clone()).await.unwrap();
    assert_eq!(miner_balance, initial_blocks as u64 * SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);

    // Get the miner UTXOs
    let utxos = rpc_client1.get_utxos_by_addresses(vec![miner_address.clone()]).await.unwrap();
    assert_eq!(utxos.len(), initial_blocks);
    for utxo in utxos.iter() {
        assert!(utxo.utxo_entry.is_coinbase);
        assert_eq!(utxo.address, Some(miner_address.clone()));
        assert_eq!(utxo.utxo_entry.amount, SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);
        assert_eq!(utxo.utxo_entry.script_public_key, miner_spk);
    }

    let mature_coinbase = utxos.iter().min_by(|x, y| x.utxo_entry.block_daa_score.cmp(&y.utxo_entry.block_daa_score)).unwrap();
    assert_eq!(mature_coinbase.utxo_entry.block_daa_score, 2);

    // Drain UTXOs changed notification channels
    clients.iter().for_each(|x| x.utxos_changed_listener.drain());

    // Spend some coins
    const TX_AMOUNT: u64 = SIMNET_PARAMS.pre_deflationary_phase_base_subsidy * 4 / 5;
    const NUMBER_OUTPUTS: u64 = 2;
    let transaction = generate_tx(
        miner_schnorr_key,
        &mature_coinbase.outpoint,
        &mature_coinbase.utxo_entry,
        TX_AMOUNT,
        NUMBER_OUTPUTS,
        &user_address,
    );
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
        let Notification::UtxosChanged(uc) = x.utxos_changed_listener.receiver.recv().await.unwrap() else {
            panic!("wrong notification type")
        };
        assert!(uc.removed.iter().any(|x| x.address.is_some() && *x.address.as_ref().unwrap() == miner_address));
        assert!(uc.added.iter().any(|x| x.address.is_some() && *x.address.as_ref().unwrap() == user_address));
        assert_eq!(uc.removed.len(), 1);
        assert_eq!(uc.added.len() as u64, NUMBER_OUTPUTS);
    }

    // Check the balance of the miner address
    let miner_balance = rpc_client2.get_balance_by_address(miner_address.clone()).await.unwrap();
    assert_eq!(miner_balance, (initial_blocks as u64 - 1) * SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);

    // Check the balance of the user address
    let user_balance = rpc_client2.get_balance_by_address(user_address.clone()).await.unwrap();
    assert_eq!(user_balance, TX_AMOUNT);

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
