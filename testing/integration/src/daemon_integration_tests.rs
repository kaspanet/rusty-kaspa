use crate::common::{
    client::ListeningClient,
    client_notify::ChannelNotify,
    daemon::Daemon,
    fee,
    utils::{fetch_spendable_utxos, mine_block, wait_for},
};
use kaspa_addresses::Address;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::params::{Params, SIMNET_GENESIS, SIMNET_PARAMS};
use kaspa_consensus_core::{
    config::params::OverrideParams,
    constants::{TX_VERSION, TX_VERSION_TOCCATA},
    header::Header,
    mass::ComputeBudget,
    sign::{sign, sign_with_multiple_v2},
    subnets::{SUBNETWORK_ID_NATIVE, SubnetworkId},
    tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{task::runtime::AsyncRuntime, trace};
use kaspa_grpc_client::GrpcClient;
use kaspa_hashes::Hash;
use kaspa_notify::{
    events::EventType,
    scope::{BlockAddedScope, UtxosChangedScope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::{Notification, RpcTransaction, RpcTransactionId, api::rpc::RpcApi};
use kaspa_txscript::{
    opcodes::codes, pay_to_address_script, pay_to_script_hash_script, pay_to_script_hash_signature_script,
    script_builder::ScriptBuilder,
};
use kaspad_lib::{
    args::Args,
    daemon::{DaemonOverrides, Runtime as KaspadRuntime},
};
use rand::thread_rng;
use serde_json;
use std::{fs, path::PathBuf, sync::Arc, time::Duration};

fn load_override_params(path: &PathBuf) -> Params {
    let override_params_json = fs::read_to_string(path).unwrap();
    let override_params: OverrideParams = serde_json::from_str(&override_params_json).unwrap();
    SIMNET_PARAMS.override_params(override_params)
}

async fn walk_parent_chain(client: &GrpcClient, mut hash: Hash, steps: u64) -> Hash {
    for _ in 0..steps {
        let block = client.get_block(hash, false).await.unwrap();
        let Some(parent) = block.header.direct_parents().first() else {
            break;
        };
        hash = *parent;
    }
    hash
}

async fn is_ancestor_in_selected_parent_chain(client: &GrpcClient, mut descendant: Hash, target: Hash) -> bool {
    loop {
        if descendant == target {
            return true;
        }
        let block = client.get_block(descendant, false).await.unwrap();
        let Some(parent) = block.header.direct_parents().first() else {
            return false;
        };
        descendant = *parent;
    }
}

// Ignored since it might fail to initialize the logger if another test already initialized it. Run it specifically with `cargo test --release --package kaspa-testing-integration --lib -- daemon_integration_tests::daemon_toccata_activation_log_file_test --ignored`
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_toccata_activation_log_file_test() {
    init_allocator_with_default_settings();

    let test_dir = tempfile::tempdir().unwrap();
    let log_dir = test_dir.path().join("logs");
    let params_path = test_dir.path().join("params.json");
    fs::create_dir_all(&log_dir).unwrap();
    fs::write(&params_path, r#"{"skip_proof_of_work":true,"toccata_activation":1}"#).unwrap();

    let args = Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        disable_upnp: true,
        disable_dns_seeding: true,
        outbound_target: 0,
        logdir: Some(log_dir.to_string_lossy().to_string()),
        override_params_file: Some(params_path.to_string_lossy().to_string()),
        ..Default::default()
    };

    let _runtime = KaspadRuntime::from_args(&args);
    let mut kaspad = Daemon::new_random_with_args(args, 10);
    let rpc_client = kaspad.start().await;
    let log_path = log_dir.join("rusty-kaspa.log");

    let initial_log = fs::read_to_string(&log_path).unwrap_or_default();
    assert!(!initial_log.contains("[Toccata] Activated for"), "Toccata activation logs were emitted before activation");

    let miner_address = Address::new(kaspad.network.into(), kaspa_addresses::Version::PubKey, &[0; 32]);
    for target_daa_score in 1..=2 {
        let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client.submit_block(template.block, false).await.unwrap();

        let activation_check_client = rpc_client.clone();
        wait_for(
            50,
            100,
            move || {
                let client = activation_check_client.clone();
                Box::pin(async move { client.get_server_info().await.unwrap().virtual_daa_score >= target_daa_score })
            },
            "daemon did not reach Toccata activation",
        )
        .await;
    }

    rpc_client.disconnect().await.unwrap();
    drop(rpc_client);
    kaspad.shutdown();

    let log = fs::read_to_string(&log_path).unwrap();
    let header_log_count = log.matches("[Toccata] Activated for header in context validation").count();
    assert_eq!(header_log_count, 1, "Toccata activation log for header in context validation should be emitted exactly once");
    let virtual_state_log_count = log.matches("[Toccata] Activated for virtual state processing rules").count();
    assert_eq!(virtual_state_log_count, 1, "Toccata activation log for virtual state processing rules should be emitted exactly once");
    assert_eq!(
        log.matches("TOCCATA").count(),
        virtual_state_log_count,
        "Toccata ASCII art should only be emitted by the virtual state logger"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_sanity_test() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    // let total_fd_limit =  kaspa_utils::fd_budget::get_limit() / 2 - 128;
    let total_fd_limit = 10;
    let mut kaspad1 = Daemon::new_random(total_fd_limit);
    let rpc_client1 = kaspad1.start().await;
    assert!(rpc_client1.handle_message_id() && rpc_client1.handle_stop_notify(), "the client failed to collect server features");

    let mut kaspad2 = Daemon::new_random(total_fd_limit);
    let rpc_client2 = kaspad2.start().await;
    assert!(rpc_client2.handle_message_id() && rpc_client2.handle_stop_notify(), "the client failed to collect server features");

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
    init_allocator_with_default_settings();
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

    let (sender, event_receiver) = async_channel::unbounded();
    rpc_client1.start(Some(Arc::new(ChannelNotify::new(sender)))).await;
    rpc_client1.start_notify(Default::default(), VirtualDaaScoreChangedScope {}.into()).await.unwrap();

    // Mine 10 blocks to daemon #1
    let mut last_block_hash = None;
    for i in 0..10 {
        let template = rpc_client1
            .get_block_template(Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &[0; 32]), vec![])
            .await
            .unwrap();
        let header: Header = (&template.block.header).try_into().unwrap();
        last_block_hash = Some(header.hash);
        rpc_client1.submit_block(template.block, false).await.unwrap();

        while let Ok(notification) = match tokio::time::timeout(Duration::from_secs(1), event_receiver.recv()).await {
            Ok(res) => res,
            Err(elapsed) => panic!("expected virtual event before {}", elapsed),
        } {
            match notification {
                Notification::VirtualDaaScoreChanged(msg) if msg.virtual_daa_score == i + 1 => {
                    break;
                }
                Notification::VirtualDaaScoreChanged(msg) if msg.virtual_daa_score > i + 1 => {
                    panic!("DAA score too high for number of submitted blocks")
                }
                Notification::VirtualDaaScoreChanged(_) => {}
                _ => panic!("expected only DAA score notifications"),
            }
        }
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    // Expect the blocks to be relayed to daemon #2
    let dag_info = rpc_client2.get_block_dag_info().await.unwrap();
    assert_eq!(dag_info.block_count, 10);
    assert_eq!(dag_info.sink, last_block_hash.unwrap());

    // Check that acceptance data contains the expected coinbase tx ids
    let vc = rpc_client2
        .get_virtual_chain_from_block(
            SIMNET_GENESIS.hash, //
            true,
            None,
        )
        .await
        .unwrap();
    assert_eq!(vc.removed_chain_block_hashes.len(), 0);
    assert_eq!(vc.added_chain_block_hashes.len(), 10);
    assert_eq!(vc.accepted_transaction_ids.len(), 10);
    for accepted_txs_pair in vc.accepted_transaction_ids {
        assert_eq!(accepted_txs_pair.accepted_transaction_ids.len(), 1);
    }
}

/// `cargo test --release --package kaspa-testing-integration --lib -- daemon_integration_tests::daemon_utxos_propagation_test`
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

    let coinbase_maturity = SIMNET_PARAMS.coinbase_maturity();
    let mut kaspad1 = Daemon::new_random_with_args(args.clone(), total_fd_limit);
    let mut kaspad2 = Daemon::new_random_with_args(args, total_fd_limit);
    let rpc_client1 = kaspad1.start().await;
    let rpc_client2 = kaspad2.start().await;

    // Let rpc_client1 receive virtual DAA score changed notifications
    let (sender1, event_receiver1) = async_channel::unbounded();
    rpc_client1.start(Some(Arc::new(ChannelNotify::new(sender1)))).await;
    rpc_client1.start_notify(Default::default(), VirtualDaaScoreChangedScope {}.into()).await.unwrap();

    // Connect kaspad2 to kaspad1
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
    let miner_spk = pay_to_address_script(&miner_address);

    // User key and address
    let (_user_sk, user_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let user_address =
        Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &user_pk.x_only_public_key().0.serialize());

    // Some dummy non-monitored address
    let blank_address = Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &[0; 32]);

    // Create a multi-listener RPC client on each node. Multi-listener subscriptions are propagated
    // upstream asynchronously, so subscribe to the streams used by mine_block before the long initial
    // mining run and later verify that notifications actually flowed through both listeners.
    let mut clients = vec![ListeningClient::connect(&kaspad2).await, ListeningClient::connect(&kaspad1).await];
    for x in clients.iter_mut() {
        x.start_notify(BlockAddedScope {}.into()).await.unwrap();
        x.start_notify(VirtualDaaScoreChangedScope {}.into()).await.unwrap();
    }

    // Mine 1000 blocks to daemon #1
    let initial_blocks = coinbase_maturity;
    let mut last_block_hash = None;
    for i in 0..initial_blocks {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        let header: Header = (&template.block.header).try_into().unwrap();
        last_block_hash = Some(header.hash);
        rpc_client1.submit_block(template.block, false).await.unwrap();

        while let Ok(notification) = match tokio::time::timeout(Duration::from_secs(1), event_receiver1.recv()).await {
            Ok(res) => res,
            Err(elapsed) => panic!("expected virtual event before {}", elapsed),
        } {
            match notification {
                Notification::VirtualDaaScoreChanged(msg) if msg.virtual_daa_score == i + 1 => {
                    break;
                }
                Notification::VirtualDaaScoreChanged(msg) if msg.virtual_daa_score > i + 1 => {
                    panic!("DAA score too high for number of submitted blocks")
                }
                Notification::VirtualDaaScoreChanged(_) => {}
                _ => panic!("expected only DAA score notifications"),
            }
        }
    }

    let check_client = rpc_client2.clone();
    wait_for(
        50,
        20,
        move || {
            async fn daa_score_reached(client: GrpcClient) -> bool {
                let virtual_daa_score = client.get_server_info().await.unwrap().virtual_daa_score;
                trace!("Virtual DAA score: {}", virtual_daa_score);
                virtual_daa_score == SIMNET_PARAMS.coinbase_maturity()
            }
            Box::pin(daa_score_reached(check_client.clone()))
        },
        "the nodes did not add and relay all the initial blocks",
    )
    .await;

    // Expect the blocks to be relayed to daemon #2
    let dag_info = rpc_client2.get_block_dag_info().await.unwrap();
    assert_eq!(dag_info.block_count, initial_blocks);
    assert_eq!(dag_info.sink, last_block_hash.unwrap());

    // Check that acceptance data contains the expected coinbase tx ids
    let vc = rpc_client2.get_virtual_chain_from_block(SIMNET_GENESIS.hash, true, None).await.unwrap();
    assert_eq!(vc.removed_chain_block_hashes.len(), 0);
    assert_eq!(vc.added_chain_block_hashes.len() as u64, initial_blocks);
    assert_eq!(vc.accepted_transaction_ids.len() as u64, initial_blocks);
    for accepted_txs_pair in vc.accepted_transaction_ids {
        assert_eq!(accepted_txs_pair.accepted_transaction_ids.len(), 1);
    }

    // Use the initial mining run as a readiness barrier for the multi-listener notification stack,
    // then consume the warm-up history through the final block and virtual DAA notifications so
    // the following mine_block calls observe only fresh notifications.
    let last_block_hash = last_block_hash.unwrap();
    let timeout_per_notification = Duration::from_secs(10);
    for x in clients.iter() {
        x.wait_for_notification(EventType::BlockAdded, timeout_per_notification, |notification| {
            matches!(notification, Notification::BlockAdded(notification) if notification.block.header.hash == last_block_hash)
        })
        .await;
        x.wait_for_notification(EventType::VirtualDaaScoreChanged, timeout_per_notification, |notification| {
            matches!(notification, Notification::VirtualDaaScoreChanged(notification) if notification.virtual_daa_score == initial_blocks)
        })
        .await;
        x.block_added_listener().unwrap().drain();
        x.virtual_daa_score_changed_listener().unwrap().drain();
    }

    // Subscribe to address-filtered UTXO notifications only after the initial maturity mining, so
    // the UTXO listener does not accumulate the 1000 coinbase notifications above.
    for x in clients.iter_mut() {
        x.start_notify(UtxosChangedScope::new(vec![miner_address.clone(), user_address.clone()]).into()).await.unwrap();
    }

    // Mine some extra blocks so the latest miner reward is added to its balance and some UTXOs reach maturity
    const EXTRA_BLOCKS: usize = 10;
    for _ in 0..EXTRA_BLOCKS {
        mine_block(blank_address.clone(), &rpc_client1, &clients).await;
    }

    // Check the balance of the miner address
    let miner_balance = rpc_client2.get_balance_by_address(miner_address.clone()).await.unwrap();
    assert_eq!(miner_balance, initial_blocks * SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);
    let miner_balance = rpc_client1.get_balance_by_address(miner_address.clone()).await.unwrap();
    assert_eq!(miner_balance, initial_blocks * SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);

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

    // Spend some coins - sending funds from miner address to user address
    // The transaction here is later used to verify utxo return address RPC
    const NUMBER_INPUTS: u64 = 2;
    const NUMBER_OUTPUTS: u64 = 2;
    const TX_AMOUNT: u64 = SIMNET_PARAMS.pre_deflationary_phase_base_subsidy * (NUMBER_INPUTS * 5 - 1) / 5;
    let selected_utxos = &utxos[0..NUMBER_INPUTS as usize];
    let tx_script_public_key = pay_to_address_script(&user_address);
    let inputs = selected_utxos
        .iter()
        .map(|(op, _)| TransactionInput {
            previous_outpoint: *op,
            signature_script: vec![],
            sequence: 0,
            compute_commit: ComputeBudget(0).into(),
        })
        .collect();
    let outputs = (0..NUMBER_OUTPUTS)
        .map(|_| TransactionOutput {
            value: TX_AMOUNT / NUMBER_OUTPUTS,
            script_public_key: tx_script_public_key.clone(),
            covenant: None,
        })
        .collect();
    let unsigned_tx = Transaction::new(TX_VERSION_TOCCATA, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signed_tx = sign_with_multiple_v2(
        MutableTransaction::with_entries(unsigned_tx, selected_utxos.iter().map(|(_, entry)| entry.clone()).collect()),
        &[miner_sk.secret_bytes()],
    )
    .unwrap();
    let mut transaction = signed_tx.tx;
    let per_input_compute_budget_commitment: u16 = 300; // ~30k-gram per-input upper bound
    transaction.inputs.iter_mut().for_each(|input| input.compute_commit = ComputeBudget(per_input_compute_budget_commitment).into());
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
        assert!(uc.removed.iter().all(|x| x.address.is_some() && *x.address.as_ref().unwrap() == miner_address));
        assert!(uc.added.iter().all(|x| x.address.is_some() && *x.address.as_ref().unwrap() == user_address));
        assert_eq!(uc.removed.len() as u64, NUMBER_INPUTS);
        assert_eq!(uc.added.len() as u64, NUMBER_OUTPUTS);
        assert_eq!(
            uc.removed.iter().map(|x| x.utxo_entry.amount).sum::<u64>(),
            SIMNET_PARAMS.pre_deflationary_phase_base_subsidy * NUMBER_INPUTS
        );
        assert_eq!(uc.added.iter().map(|x| x.utxo_entry.amount).sum::<u64>(), TX_AMOUNT);
    }

    // Check the balance of both miner and user addresses
    for x in clients.iter() {
        let miner_balance = x.get_balance_by_address(miner_address.clone()).await.unwrap();
        assert_eq!(miner_balance, (initial_blocks - NUMBER_INPUTS) * SIMNET_PARAMS.pre_deflationary_phase_base_subsidy);

        let user_balance = x.get_balance_by_address(user_address.clone()).await.unwrap();
        assert_eq!(user_balance, TX_AMOUNT);
    }

    // UTXO Return Address Test
    // Mine another block to accept the transactions from the previous block
    // The tx above is sending from miner address to user address
    mine_block(blank_address.clone(), &rpc_client1, &clients).await;
    let new_utxos = rpc_client1.get_utxos_by_addresses(vec![user_address]).await.unwrap();
    let new_utxo = new_utxos
        .iter()
        .find(|utxo| utxo.outpoint.transaction_id == transaction.id())
        .expect("Did not find a utxo for the tx we just created but expected to");

    let utxo_return_address = rpc_client1
        .get_utxo_return_address(new_utxo.outpoint.transaction_id, new_utxo.utxo_entry.block_daa_score)
        .await
        .expect("We just created the tx and utxo here");

    assert_eq!(miner_address, utxo_return_address);

    // Terminate multi-listener clients
    for x in clients.iter() {
        x.disconnect().await.unwrap();
        x.join().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_compute_budget_relay_test() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let compute_budget_relay_test_params =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/params/compute_budget_relay_test_params.json");

    let args = Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        disable_upnp: true,
        disable_dns_seeding: true,
        utxoindex: true,
        outbound_target: 0,
        override_params_file: Some(compute_budget_relay_test_params.to_string_lossy().to_string()),
        ..Default::default()
    };
    let total_fd_limit = 10;

    let coinbase_maturity = 0;
    let mut kaspad1 = Daemon::new_random_with_args(args.clone(), total_fd_limit);
    let mut kaspad2 = Daemon::new_random_with_args(args, total_fd_limit);
    let rpc_client1 = kaspad1.start().await;
    let rpc_client2 = kaspad2.start().await;

    rpc_client2.add_peer(format!("127.0.0.1:{}", kaspad1.p2p_port).try_into().unwrap(), true).await.unwrap();
    let check_client = rpc_client2.clone();
    wait_for(
        50,
        40,
        move || {
            async fn peer_connected(client: GrpcClient) -> bool {
                client.get_connected_peer_info().await.unwrap().peer_info.len() == 1
            }
            Box::pin(peer_connected(check_client.clone()))
        },
        "the nodes did not connect to each other",
    )
    .await;

    let (miner_sk, miner_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let miner_address =
        Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &miner_pk.x_only_public_key().0.serialize());
    let (_user_sk, user_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let user_address =
        Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &user_pk.x_only_public_key().0.serialize());

    let mut last_block_hash = None;
    for _ in 0..coinbase_maturity {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        let header: Header = (&template.block.header).try_into().unwrap();
        last_block_hash = Some(header.hash);
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    if let Some(expected_sink) = last_block_hash {
        let check_client = rpc_client2.clone();
        wait_for(
            50,
            40,
            move || {
                async fn node_synced(client: GrpcClient, expected_sink: Hash) -> bool {
                    let info = client.get_block_dag_info().await.unwrap();
                    info.sink == expected_sink
                }
                Box::pin(node_synced(check_client.clone(), expected_sink))
            },
            "node #2 did not sync to node #1 tip",
        )
        .await;
    }

    const EXTRA_BLOCKS: usize = 10;
    for _ in 0..EXTRA_BLOCKS {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    let expected_sink = rpc_client1.get_block_dag_info().await.unwrap().sink;
    let check_client = rpc_client2.clone();
    wait_for(
        50,
        200,
        move || {
            async fn node_synced(client: GrpcClient, expected_sink: Hash) -> bool {
                client.get_block_dag_info().await.unwrap().sink == expected_sink
            }
            Box::pin(node_synced(check_client.clone(), expected_sink))
        },
        "node #2 did not catch up after extra blocks",
    )
    .await;

    if rpc_client2.get_connected_peer_info().await.unwrap().peer_info.is_empty() {
        rpc_client2.add_peer(format!("127.0.0.1:{}", kaspad1.p2p_port).try_into().unwrap(), true).await.unwrap();
        let check_client = rpc_client2.clone();
        wait_for(
            50,
            200,
            move || {
                async fn peer_connected(client: GrpcClient) -> bool {
                    client.get_connected_peer_info().await.unwrap().peer_info.len() == 1
                }
                Box::pin(peer_connected(check_client.clone()))
            },
            "the nodes were disconnected before transaction submission",
        )
        .await;
    }

    let check_client1 = rpc_client1.clone();
    let check_client2 = rpc_client2.clone();
    wait_for(
        50,
        600,
        move || {
            async fn tips_aligned(client1: GrpcClient, client2: GrpcClient) -> bool {
                let tip1 = client1.get_block_dag_info().await.unwrap().sink;
                let tip2 = client2.get_block_dag_info().await.unwrap().sink;
                tip1 == tip2
            }
            Box::pin(tips_aligned(check_client1.clone(), check_client2.clone()))
        },
        "the nodes did not align to the same tip before transaction submission",
    )
    .await;

    let utxos = fetch_spendable_utxos(&rpc_client1, miner_address.clone(), coinbase_maturity).await;
    const NUMBER_INPUTS: u64 = 2;
    const NUMBER_OUTPUTS: u64 = 2;
    const PER_INPUT_COMPUTE_BUDGET: u16 = 30;
    const EXTRA_FEE: u64 = 10_000;
    let oldest_utxos_start = utxos.len() - NUMBER_INPUTS as usize;
    let selected_utxos = &utxos[oldest_utxos_start..];
    let total_in = selected_utxos.iter().map(|x| x.1.amount).sum::<u64>();
    let script_public_key = pay_to_address_script(&user_address);
    let build_transaction = |tx_output_amount: u64| {
        let inputs = selected_utxos
            .iter()
            .map(|(op, _)| TransactionInput {
                previous_outpoint: *op,
                signature_script: vec![],
                sequence: 0,
                compute_commit: ComputeBudget(0).into(),
            })
            .collect();
        let outputs = (0..NUMBER_OUTPUTS)
            .map(|_| TransactionOutput {
                value: tx_output_amount / NUMBER_OUTPUTS,
                script_public_key: script_public_key.clone(),
                covenant: None,
            })
            .collect();
        let unsigned_tx = Transaction::new(TX_VERSION_TOCCATA, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        sign_with_multiple_v2(
            MutableTransaction::with_entries(unsigned_tx, selected_utxos.iter().map(|(_, entry)| entry.clone()).collect()),
            &[miner_sk.secret_bytes()],
        )
        .unwrap()
        .tx
    };

    let tx_fee = fee::calc_from_probe(|| {
        let mut tx = build_transaction(total_in);
        tx.inputs.iter_mut().for_each(|input| input.compute_commit = ComputeBudget(PER_INPUT_COMPUTE_BUDGET).into());
        tx
    })
    .saturating_add(EXTRA_FEE);
    let tx_amount = total_in.checked_sub(tx_fee).expect("expected enough input value for test transaction fee");

    let mut transaction = build_transaction(tx_amount);
    transaction.inputs.iter_mut().for_each(|input| input.compute_commit = ComputeBudget(PER_INPUT_COMPUTE_BUDGET).into());
    assert!(
        transaction.inputs.iter().any(|input| input.compute_commit.compute_budget().unwrap() > 0),
        "expected non-zero compute_budget commitment for v1 transaction"
    );
    let transaction_id = transaction.id();
    rpc_client1.submit_transaction((&transaction).into(), false).await.unwrap();

    let check_client = rpc_client1.clone();
    wait_for(
        50,
        200,
        move || {
            async fn transaction_in_mempool(client: GrpcClient, transaction_id: RpcTransactionId) -> bool {
                client.get_mempool_entry(transaction_id, false, false).await.is_ok()
            }
            Box::pin(transaction_in_mempool(check_client.clone(), transaction_id))
        },
        "the transaction was not added to node #1 mempool",
    )
    .await;

    let node1_entry = rpc_client1.get_mempool_entry(transaction_id, false, false).await.unwrap();
    assert_eq!(node1_entry.transaction.version, TX_VERSION_TOCCATA);
    let node1_compute_budget = node1_entry.transaction.inputs[0].compute_budget;
    assert!(node1_compute_budget > 0, "expected non-zero compute_budget on node #1 mempool tx");

    let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
    let mined_header: Header = (&template.block.header).try_into().unwrap();
    let mined_block_hash = mined_header.hash;
    rpc_client1.submit_block(template.block, false).await.unwrap();

    let check_client = rpc_client2.clone();
    wait_for(
        50,
        200,
        move || {
            async fn node_synced(client: GrpcClient, expected_sink: Hash) -> bool {
                client.get_block_dag_info().await.unwrap().sink == expected_sink
            }
            Box::pin(node_synced(check_client.clone(), mined_block_hash))
        },
        "node #2 did not receive the mined block with the transaction",
    )
    .await;

    let block2 = rpc_client2.get_block(mined_block_hash, true).await.unwrap();
    let included_tx = block2
        .transactions
        .iter()
        .find(|tx| tx.verbose_data.as_ref().is_some_and(|vd| vd.transaction_id == transaction_id))
        .expect("node #2 block does not include the submitted transaction");

    assert_eq!(included_tx.version, TX_VERSION_TOCCATA);
    let included_compute_budget = included_tx.inputs[0].compute_budget;
    assert!(included_compute_budget > 0, "expected non-zero compute_budget on propagated block tx");
    assert_eq!(included_compute_budget, node1_compute_budget);

    rpc_client1.disconnect().await.unwrap();
    rpc_client2.disconnect().await.unwrap();
    kaspad1.shutdown();
    kaspad2.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_rejects_transactions_with_inconsistent_input_mass_and_version() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    let compute_budget_relay_test_params =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/params/compute_budget_relay_test_params.json");
    let args = Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        disable_upnp: true,
        disable_dns_seeding: true,
        utxoindex: true,
        outbound_target: 0,
        override_params_file: Some(compute_budget_relay_test_params.to_string_lossy().to_string()),
        ..Default::default()
    };

    let mut kaspad = Daemon::new_random_with_args(args, 10);
    let rpc_client = kaspad.start().await;

    let (miner_sk, miner_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let miner_address =
        Address::new(kaspad.network.into(), kaspa_addresses::Version::PubKey, &miner_pk.x_only_public_key().0.serialize());
    let pay_spk = pay_to_address_script(&miner_address);
    let miner_schnorr_key = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &miner_sk);

    for _ in 0..4 {
        let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client.submit_block(template.block, false).await.unwrap();
    }

    let utxos = fetch_spendable_utxos(&rpc_client, miner_address.clone(), 0).await;
    assert!(utxos.len() >= 2, "expected enough spendable UTXOs for malformed transaction tests");

    let build_single_input_tx = |version: u16, selected_utxo: &(TransactionOutpoint, UtxoEntry)| {
        let fee = fee::calc_for_plain_standard_tx(1, 1);
        let output_value = selected_utxo.1.amount.checked_sub(fee).expect("expected enough input value for test fee");
        let compute_commit = ComputeBudget(0).into(); // set correctly by sign below
        let tx = Transaction::new(
            version,
            vec![TransactionInput { previous_outpoint: selected_utxo.0, signature_script: vec![], sequence: 0, compute_commit }],
            vec![TransactionOutput { value: output_value, script_public_key: pay_spk.clone(), covenant: None }],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        sign(MutableTransaction::with_entries(tx, vec![selected_utxo.1.clone()]), miner_schnorr_key).tx
    };

    let v1_tx = build_single_input_tx(TX_VERSION_TOCCATA, &utxos[0]);
    let valid_v1_rpc_tx: RpcTransaction = (&v1_tx).into();
    let mut malformed_v1_rpc_tx = valid_v1_rpc_tx.clone();
    malformed_v1_rpc_tx.inputs[0].sig_op_count = 1;
    assert!(
        rpc_client.submit_transaction(malformed_v1_rpc_tx, false).await.is_err(),
        "expected v1 transaction with non-zero sig_op_count to be rejected at the daemon boundary"
    );

    let v0_tx = build_single_input_tx(TX_VERSION, &utxos[1]);
    let valid_v0_rpc_tx: RpcTransaction = (&v0_tx).into();
    let mut malformed_v0_rpc_tx: RpcTransaction = valid_v0_rpc_tx.clone();
    malformed_v0_rpc_tx.inputs[0].compute_budget = 1;
    assert!(
        rpc_client.submit_transaction(malformed_v0_rpc_tx, false).await.is_err(),
        "expected v0 transaction with non-zero compute_budget to be rejected at the daemon boundary"
    );

    rpc_client.submit_transaction(valid_v1_rpc_tx, false).await.expect("expected the valid v1 transaction to be accepted");
    rpc_client.submit_transaction(valid_v0_rpc_tx, false).await.expect("expected the valid v0 transaction to be accepted");

    rpc_client.disconnect().await.unwrap();
    kaspad.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_pruning_seqcommit_sync_test() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO,kaspa_testing_integration=trace,kaspa_rpc_core=debug");

    let override_params_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/params/seqcommit_sync_test_params.json");
    let params = load_override_params(&override_params_path);

    let args = Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        disable_upnp: true,
        utxoindex: true,
        override_params_file: Some(override_params_path.to_string_lossy().to_string()),
        ..Default::default()
    };

    let total_fd_limit = 10;
    let mut kaspad1 = Daemon::new_random_with_args(args.clone(), total_fd_limit);
    let rpc_client1 = kaspad1.start().await;

    let (miner_sk, miner_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let miner_address =
        Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &miner_pk.x_only_public_key().0.serialize());
    let miner_schnorr_key = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &miner_sk);

    // Step 1: advance the chain to ~1.5 * finality depth from genesis.
    // We will create a seqcommit transaction at that height, referencing a block
    // almost a full finality_depth below the tip (KIP-21 seqcommit look-back is
    // bounded by `finality_depth`).
    let finality_depth = params.finality_depth();
    let initial_blocks = finality_depth.saturating_mul(3).saturating_div(2) as usize;
    for _ in 0..initial_blocks {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    let mined_check = rpc_client1.clone();
    wait_for(
        50,
        40,
        move || {
            let client = mined_check.clone();
            Box::pin(async move { client.get_server_info().await.unwrap().virtual_daa_score >= initial_blocks as u64 })
        },
        "syncer did not reach the initial finality depth target",
    )
    .await;

    // Choose a target almost a full finality_depth below the current tip, leaving
    // a small buffer for the confirmation and spend blocks.
    let dag_info = rpc_client1.get_block_dag_info().await.unwrap();
    let remaining = finality_depth.saturating_sub(3);
    let target_block = walk_parent_chain(&rpc_client1, dag_info.sink, remaining).await;

    // Build a P2SH redeem script that exercises OpChainblockSeqCommit.
    let mut builder = ScriptBuilder::new();
    builder.add_data(&target_block.as_bytes()).unwrap();
    builder.add_op(codes::OpChainblockSeqCommit).unwrap();
    builder.add_op(codes::OpDrop).unwrap();
    builder.add_op(codes::OpTrue).unwrap();
    let redeem_script = builder.drain();
    let seqcommit_spk = pay_to_script_hash_script(&redeem_script);

    // Fund the P2SH output and confirm it on the syncer at ~1.5 * finality depth.
    let utxos = fetch_spendable_utxos(&rpc_client1, miner_address.clone(), 10).await;
    let input_utxos = &utxos[0..1];
    let total_in = input_utxos.iter().map(|x| x.1.amount).sum::<u64>();
    let fee = fee::calc_for_plain_standard_tx(input_utxos.len(), 1);
    let outputs = vec![TransactionOutput { value: total_in - fee, script_public_key: seqcommit_spk.clone(), covenant: None }];
    let inputs = input_utxos.iter().map(|(op, _)| TransactionInput::new(*op, vec![], 0, 1)).collect();
    let unsigned_tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signed_tx =
        sign(MutableTransaction::with_entries(unsigned_tx, input_utxos.iter().map(|(_, e)| e.clone()).collect()), miner_schnorr_key);
    let seqcommit_tx = signed_tx.tx.clone();
    rpc_client1.submit_transaction((&seqcommit_tx).into(), false).await.unwrap();

    let mempool_check = rpc_client1.clone();
    let seqcommit_tx_id = seqcommit_tx.id();
    wait_for(
        50,
        20,
        move || {
            let client = mempool_check.clone();
            Box::pin(async move { client.get_mempool_entry(seqcommit_tx_id, false, false).await.is_ok() })
        },
        "seqcommit transaction was not added to the mempool",
    )
    .await;

    let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
    rpc_client1.submit_block(template.block, false).await.unwrap();

    // Spend the P2SH output to trigger seqcommit validation on the syncer while the target is
    // still within the finality depth of the spending block.
    let outpoint = TransactionOutpoint::new(seqcommit_tx.id(), 0);
    let pay_spk = pay_to_address_script(&miner_address);
    let signature_script = pay_to_script_hash_signature_script(redeem_script, vec![]).expect("canonical signature script");
    let spend_fee = fee::calc_for_plain_standard_tx_with_extra_serialized_bytes(1, 1, signature_script.len() as u64);
    let spend_value = total_in - fee - spend_fee;
    let spend_tx = Transaction::new(
        TX_VERSION,
        vec![TransactionInput::new(outpoint, signature_script, 0, 1)],
        vec![TransactionOutput { value: spend_value, script_public_key: pay_spk, covenant: None }],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    rpc_client1.submit_transaction((&spend_tx).into(), false).await.unwrap();

    let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
    rpc_client1.submit_block(template.block, false).await.unwrap();

    // Step 2: advance the pruning point so it moves off genesis and ends up above the
    // target block that the seqcommit script references.
    //
    // KIP-21: the seqcommit look-back is `finality_depth`, so the target sits at
    // depth ≈ F below the initial tip. Pruning samples space by F in blue_score, so
    // PP may need to advance to climb past the target.
    let mut dag_info = rpc_client1.get_block_dag_info().await.unwrap();
    let mut extra_blocks = 0usize;
    let extra_blocks_limit = params.pruning_depth().saturating_add(params.finality_depth()).saturating_add(30) as usize;
    while (dag_info.pruning_point_hash == SIMNET_GENESIS.hash
        || !is_ancestor_in_selected_parent_chain(&rpc_client1, dag_info.pruning_point_hash, target_block).await)
        && extra_blocks < extra_blocks_limit
    {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client1.submit_block(template.block, false).await.unwrap();
        extra_blocks += 1;
        dag_info = rpc_client1.get_block_dag_info().await.unwrap();
    }
    if dag_info.pruning_point_hash == SIMNET_GENESIS.hash {
        panic!("pruning point did not advance from genesis in time");
    }
    if !is_ancestor_in_selected_parent_chain(&rpc_client1, dag_info.pruning_point_hash, target_block).await {
        panic!("pruning point did not advance above the seqcommit target in time");
    }

    // Step 3: only now start the syncee and let it sync and validate the seqcommit flow.
    let mut kaspad2 = Daemon::new_random_with_args(args, total_fd_limit);
    let rpc_client2 = kaspad2.start().await;

    rpc_client2.add_peer(format!("127.0.0.1:{}", kaspad1.p2p_port).try_into().unwrap(), true).await.unwrap();
    let check_client = rpc_client2.clone();
    wait_for(
        50,
        40,
        move || {
            let client = check_client.clone();
            Box::pin(async move { client.get_connected_peer_info().await.unwrap().peer_info.len() == 1 })
        },
        "the nodes did not connect to each other",
    )
    .await;

    let sync_check = rpc_client2.clone();
    let target_daa_score = rpc_client1.get_server_info().await.unwrap().virtual_daa_score;
    wait_for(
        100,
        60,
        move || {
            let client = sync_check.clone();
            Box::pin(async move { client.get_server_info().await.unwrap().virtual_daa_score >= target_daa_score })
        },
        "syncee did not complete IBD",
    )
    .await;

    // The spend block is already mined before the pruning point moves, so the syncee
    // should validate it while syncing historical data.

    let synced_check = rpc_client2.clone();
    let final_score = rpc_client1.get_server_info().await.unwrap().virtual_daa_score;
    wait_for(
        100,
        40,
        move || {
            let client = synced_check.clone();
            Box::pin(async move { client.get_server_info().await.unwrap().virtual_daa_score >= final_score })
        },
        "syncee did not accept seqcommit block",
    )
    .await;

    rpc_client1.disconnect().await.unwrap();
    rpc_client2.disconnect().await.unwrap();
    kaspad1.shutdown();
    kaspad2.shutdown();
}

// IBD test focused on `sync_new_smt_state` (protocol/flows/src/ibd/flow.rs:635).
// Produces a non-trivial active-lanes SMT by submitting one transaction per
// distinct subnetwork_id — each distinct subnetwork_id creates a new lane (see
// consensus/src/pipeline/virtual_processor/utxo_validation.rs:532). With the
// `test-smt-small-chunks` feature active the stream uses SMT_CHUNK_SIZE=4 and
// SMT_FLOW_CONTROL_WINDOW=2, so `SMT_LANE_COUNT = 10` forces 3 chunks and one
// flow-control round-trip — exercising both chunked streaming and the
// RequestNextPruningPointSmtChunk handshake end to end.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_ibd_smt_state_sync_test() {
    const SMT_LANE_COUNT: usize = 10;
    const SMT_ANTICONE_COUNT: usize = 4;

    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO,kaspa_testing_integration=trace,kaspa_rpc_core=debug");

    let override_params_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/params/seqcommit_sync_test_params.json");
    let params = load_override_params(&override_params_path);

    let args = Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        disable_upnp: true,
        utxoindex: true,
        override_params_file: Some(override_params_path.to_string_lossy().to_string()),
        ..Default::default()
    };

    let total_fd_limit = 10;
    let mut kaspad1 = Daemon::new_random_with_args(args.clone(), total_fd_limit);
    let rpc_client1 = kaspad1.start().await;

    let (miner_sk, miner_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let miner_address =
        Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &miner_pk.x_only_public_key().0.serialize());
    let miner_schnorr_key = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &miner_sk);

    // Phase 1: mine enough blocks to mature SMT_LANE_COUNT coinbase outputs.
    let coinbase_maturity = params.coinbase_maturity();
    let initial_blocks = (coinbase_maturity as usize) + SMT_LANE_COUNT + 20;
    for _ in 0..initial_blocks {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    let mined_check = rpc_client1.clone();
    wait_for(
        50,
        60,
        move || {
            let client = mined_check.clone();
            Box::pin(async move { client.get_server_info().await.unwrap().virtual_daa_score >= initial_blocks as u64 })
        },
        "syncer did not reach the initial mining target",
    )
    .await;

    let mut anticone_templates = Vec::with_capacity(SMT_ANTICONE_COUNT);
    for i in 0..SMT_ANTICONE_COUNT {
        let extra = format!("anticone-{i:02}").into_bytes();
        anticone_templates.push(rpc_client1.get_block_template(miner_address.clone(), extra).await.unwrap());
    }
    for template in anticone_templates {
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }
    // Bury the siblings under a few chain blocks so virtual's selected
    // parent is past them when the lane txs come in.
    for _ in 0..10 {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    // Phase 2: submit SMT_LANE_COUNT transactions, each on a distinct non-reserved
    // subnetwork_id, so every one populates a fresh lane in the active-lanes SMT.
    let utxos = fetch_spendable_utxos(&rpc_client1, miner_address.clone(), coinbase_maturity).await;
    assert!(utxos.len() >= SMT_LANE_COUNT, "syncer produced {} spendable utxos, need {}", utxos.len(), SMT_LANE_COUNT);

    let mut submitted_tx_ids: Vec<RpcTransactionId> = Vec::with_capacity(SMT_LANE_COUNT);
    for (i, (outpoint, entry)) in utxos.iter().take(SMT_LANE_COUNT).enumerate() {
        // Post-HF user-lane shape is `[namespace (4 bytes), 0×16]` with a
        // non-zero byte somewhere in bytes[1..4] (see
        // consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs).
        // A distinct nonzero byte at position 3 keeps each lane_id unique while
        // conforming to the shape.
        let mut subnet_bytes = [0u8; 20];
        subnet_bytes[3] = (i as u8) + 1;
        let lane_subnet = SubnetworkId::from_bytes(subnet_bytes);

        let fee = fee::calc_for_plain_standard_tx(1, 1);
        assert!(entry.amount > fee, "coinbase utxo is too small to cover a tx fee");
        let out_value = entry.amount - fee;
        let unsigned_tx = Transaction::new(
            TX_VERSION_TOCCATA,
            vec![TransactionInput::new(*outpoint, vec![], 0, 1)],
            vec![TransactionOutput { value: out_value, script_public_key: pay_to_address_script(&miner_address), covenant: None }],
            0,
            lane_subnet,
            0,
            vec![],
        );
        let signed_tx = sign(MutableTransaction::with_entries(unsigned_tx, vec![entry.clone()]), miner_schnorr_key);
        let tx_id = signed_tx.tx.id();
        rpc_client1.submit_transaction((&signed_tx.tx).into(), false).await.unwrap();
        submitted_tx_ids.push(tx_id);
    }

    let mempool_check = rpc_client1.clone();
    let expected_ids = submitted_tx_ids.clone();
    wait_for(
        50,
        40,
        move || {
            let client = mempool_check.clone();
            let ids = expected_ids.clone();
            Box::pin(async move {
                for id in &ids {
                    if client.get_mempool_entry(*id, false, false).await.is_err() {
                        return false;
                    }
                }
                true
            })
        },
        "lane transactions did not reach the mempool",
    )
    .await;

    // Phase 3: mine enough additional blocks that the lane transactions land on
    // chain and the pruning point then advances off genesis. `pruning_depth` + a
    // comfortable margin guarantees the pruning point covers the lane txs.
    let pruning_depth = params.pruning_depth();
    let blocks_after_txs = pruning_depth as usize + 60;
    for _ in 0..blocks_after_txs {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    let mut dag_info = rpc_client1.get_block_dag_info().await.unwrap();
    let mut extra_blocks = 0usize;
    while dag_info.pruning_point_hash == SIMNET_GENESIS.hash && extra_blocks < 100 {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client1.submit_block(template.block, false).await.unwrap();
        extra_blocks += 1;
        dag_info = rpc_client1.get_block_dag_info().await.unwrap();
    }
    assert_ne!(dag_info.pruning_point_hash, SIMNET_GENESIS.hash, "syncer pruning point did not advance off genesis");

    let target_daa_score = rpc_client1.get_server_info().await.unwrap().virtual_daa_score;
    let target_pruning_point = dag_info.pruning_point_hash;

    // Phase 4: bring up the syncee and connect it to the syncer.
    let mut kaspad2 = Daemon::new_random_with_args(args, total_fd_limit);
    let rpc_client2 = kaspad2.start().await;

    rpc_client2.add_peer(format!("127.0.0.1:{}", kaspad1.p2p_port).try_into().unwrap(), true).await.unwrap();
    let check_client = rpc_client2.clone();
    wait_for(
        50,
        40,
        move || {
            let client = check_client.clone();
            Box::pin(async move { client.get_connected_peer_info().await.unwrap().peer_info.len() == 1 })
        },
        "the nodes did not connect to each other",
    )
    .await;

    // Phase 5: wait for IBD (including `sync_new_smt_state`) to complete
    let sync_check = rpc_client2.clone();
    wait_for(
        100,
        600,
        move || {
            let client = sync_check.clone();
            Box::pin(async move {
                let server_info = client.get_server_info().await.unwrap();
                if server_info.virtual_daa_score < target_daa_score {
                    return false;
                }
                client.get_block_dag_info().await.unwrap().pruning_point_hash == target_pruning_point
            })
        },
        "syncee did not complete SMT-era IBD within timeout (suspected sync_new_smt_state stall)",
    )
    .await;

    // Phase 6: mine finality_depth + buffer blocks on the syncer and assert
    // the syncee catches up. Verifies syncer/syncee shortcut agreement for live
    // blocks whose target_bs lands in the IBD-imported lane range.
    let finality_depth = params.finality_depth() as usize;
    let post_ibd_blocks = finality_depth + 30;
    for _ in 0..post_ibd_blocks {
        let template = rpc_client1.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    let post_ibd_target_score = rpc_client1.get_server_info().await.unwrap().virtual_daa_score;
    let post_ibd_target_pp = rpc_client1.get_block_dag_info().await.unwrap().pruning_point_hash;
    let post_ibd_check = rpc_client2.clone();
    wait_for(
        100,
        600,
        move || {
            let client = post_ibd_check.clone();
            Box::pin(async move {
                let server_info = client.get_server_info().await.unwrap();
                if server_info.virtual_daa_score < post_ibd_target_score {
                    return false;
                }
                client.get_block_dag_info().await.unwrap().pruning_point_hash == post_ibd_target_pp
            })
        },
        "syncee did not accept post-IBD blocks",
    )
    .await;

    rpc_client1.disconnect().await.unwrap();
    rpc_client2.disconnect().await.unwrap();
    kaspad1.shutdown();
    kaspad2.shutdown();
}

// The following test runtime parameters are required for a graceful shutdown of the gRPC server
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn daemon_cleaning_test() {
    init_allocator_with_default_settings();
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
        consensus_manager = Arc::downgrade(&Arc::downcast::<ConsensusManager>(dyn_consensus_manager.into_any_arc()).unwrap());
        async_runtime = Arc::downgrade(&Arc::downcast::<AsyncRuntime>(dyn_async_runtime.into_any_arc()).unwrap());
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

// =============================================================================
// Hostname endpoint integration tests
// =============================================================================

/// `--addpeer=localhost:<port>` parses, resolves via the OS resolver, and
/// kaspad starts cleanly with the resulting socket addresses staged in the
/// connection request set.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn kaspad_addpeer_hostname_localhost_starts() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    use kaspa_utils::networking::PeerEndpoint;
    let args = Args {
        devnet: true,
        disable_upnp: true,
        add_peers: vec![PeerEndpoint::from_str("localhost").expect("parse hostname")],
        hostname_refresh_interval_sec: 0,
        ..Default::default()
    };
    let total_fd_limit = 10;
    let mut kaspad = Daemon::new_random_with_args(args, total_fd_limit);
    let rpc_client = kaspad.start().await;
    // If startup made it this far, hostname resolution succeeded and the
    // node is up. A single round-trip RPC confirms the gRPC server reached
    // the steady state.
    assert!(rpc_client.handle_message_id(), "client did not collect server features after addpeer hostname startup");
    rpc_client.disconnect().await.unwrap();
    drop(rpc_client);
    kaspad.shutdown();
}

/// `--addpeer=127.0.0.1:<port>` (numeric IPv4 literal) takes the same
/// short-circuit path as before the hostname work landed; this is the
/// IP-only regression guard.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn kaspad_addpeer_ipv4_unchanged() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    use kaspa_utils::networking::PeerEndpoint;
    let args = Args {
        devnet: true,
        disable_upnp: true,
        add_peers: vec![PeerEndpoint::from_str("127.0.0.1:12345").unwrap()],
        hostname_refresh_interval_sec: 0,
        ..Default::default()
    };
    let mut kaspad = Daemon::new_random_with_args(args, 10);
    let rpc_client = kaspad.start().await;
    assert!(rpc_client.handle_message_id());
    rpc_client.disconnect().await.unwrap();
    drop(rpc_client);
    kaspad.shutdown();
}

/// `--addpeer=[::1]:<port>` (numeric IPv6 literal) regresses identically.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn kaspad_addpeer_ipv6_unchanged() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    use kaspa_utils::networking::PeerEndpoint;
    let args = Args {
        devnet: true,
        disable_upnp: true,
        add_peers: vec![PeerEndpoint::from_str("[::1]:12345").unwrap()],
        hostname_refresh_interval_sec: 0,
        ..Default::default()
    };
    let mut kaspad = Daemon::new_random_with_args(args, 10);
    let rpc_client = kaspad.start().await;
    assert!(rpc_client.handle_message_id());
    rpc_client.disconnect().await.unwrap();
    drop(rpc_client);
    kaspad.shutdown();
}

/// `--hostname-refresh-interval=0` is honored: the connection manager
/// instantiates without a periodic refresh task. Verified indirectly by
/// successful startup with a hostname endpoint and `interval=0`.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn kaspad_periodic_refresh_disabled_with_zero_interval() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    use kaspa_utils::networking::PeerEndpoint;
    let args = Args {
        devnet: true,
        disable_upnp: true,
        add_peers: vec![PeerEndpoint::from_str("localhost").unwrap()],
        hostname_refresh_interval_sec: 0,
        ..Default::default()
    };
    let mut kaspad = Daemon::new_random_with_args(args, 10);
    let rpc_client = kaspad.start().await;
    assert!(rpc_client.handle_message_id());
    rpc_client.disconnect().await.unwrap();
    drop(rpc_client);
    kaspad.shutdown();
}

// FromStr is used for the hostname endpoints in the tests above.
use std::str::FromStr;

/// `--addpeer=<unresolvable-host>` does NOT abort kaspad; the hostname is
/// registered for periodic retry, the `initial_failed` metric is bumped,
/// and the daemon keeps serving normally. The unresolvable-host path and
/// the unreachable-IP path both queue the entry and retry forever, never
/// refusing startup.
///
/// Source: https://github.com/bitcoin/bitcoin/blob/8f4a3ba8972dae9412ba975a040cea22c227f983/src/net.cpp#L2974
/// (`ThreadOpenAddedConnections`).
///
/// The fake resolver returns `Err` for the cited hostname so no real DNS
/// is consulted. The assertion stack is: (1) `Daemon::new_random_with_args`
/// + `start()` complete without panicking - i.e. `create_core_with_runtime`
/// returned a live daemon despite the unresolvable peer endpoint;
/// (2) the metric counter `initial_failed >= 1` (registered by the
/// connection manager when the resolver returned `Err`); (3) the daemon
/// is still alive after `2 x hostname_refresh_interval` (a healthy gRPC
/// round-trip against the running RPC server is the strongest single
/// liveness probe available in-process).
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn kaspad_addpeer_hostname_unresolvable_keeps_running() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    use kaspa_connectionmanager::test_support::FakeHostnameResolver;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_utils::networking::PeerEndpoint;

    let host = "nonexistent.kas947.invalid";
    let endpoint = PeerEndpoint::from_str(host).expect("parse hostname endpoint");
    let resolver = Arc::new(FakeHostnameResolver::new());
    // The connection manager hands the active network's default p2p port
    // to the resolver when the endpoint omits one. Derive it from the
    // `kaspa-consensus-core` `NetworkId` API so a future port reshuffle on
    // the consensus side does not silently regress the test to "resolver
    // never called" while assertions still pass with `call_count = 0`.
    let devnet_p2p_port = NetworkId::new(NetworkType::Devnet).default_p2p_port();
    resolver.set_err(host, devnet_p2p_port, "fake resolver: nonexistent.kas947.invalid does not resolve");

    let refresh_interval_sec = 2u64;
    let args = Args {
        devnet: true,
        disable_upnp: true,
        add_peers: vec![endpoint],
        hostname_refresh_interval_sec: refresh_interval_sec,
        ..Default::default()
    };
    let overrides = DaemonOverrides { hostname_resolver: Some(resolver.clone()) };
    let mut kaspad = Daemon::new_random_with_args_and_overrides(args, overrides, 10);
    // start() runs the bound services. With the post-amendment
    // register-on-failure design, this returns a working RPC client even
    // though the only `--addpeer` host is unresolvable.
    let rpc_client = kaspad.start().await;

    // Liveness probe: the gRPC server is reachable and responsive. If the
    // daemon had aborted, `start()` would have hung or panicked first.
    assert!(
        rpc_client.handle_message_id(),
        "RPC client did not collect server features: kaspad must keep running on unresolvable --addpeer",
    );

    // Wait at least 2 x refresh_interval so the periodic refresh task has
    // had room to tick at least twice past the initial registration.
    tokio::time::sleep(Duration::from_secs(2 * refresh_interval_sec + 1)).await;
    let snapshot =
        kaspad.hostname_metrics_snapshot().await.expect("connection manager should be wired into the flow context after start()");
    assert!(
        snapshot.resolutions_total.initial_failed >= 1,
        "expected initial_failed >= 1 after registering an unresolvable hostname; snapshot = {snapshot:?}; resolver call_count = {}",
        resolver.call_count(),
    );
    assert_eq!(snapshot.resolutions_total.initial_ok, 0, "no successful initial resolution expected; snapshot = {snapshot:?}",);
    // The daemon stayed up across the observation window. A second
    // RPC round-trip confirms it is still serving requests.
    assert!(rpc_client.handle_message_id(), "RPC client lost server liveness during the observation window");

    rpc_client.disconnect().await.unwrap();
    drop(rpc_client);
    kaspad.shutdown();
}

/// Companion to `kaspad_addpeer_hostname_unresolvable_keeps_running`:
/// drives `PeerEndpointResolveError::Timeout` deterministically through
/// the seam that
/// [`kaspa_connectionmanager::test_support::FakeHostnameResolver::set_timeout`]
/// exposes, and verifies the timeout-arm metric increment lands in the
/// `initial_failed` bucket.
///
/// The production [`kaspa_connectionmanager::TokioHostnameResolver`]
/// only emits this variant under a real wall-clock timeout (`~5 s` per
/// resolve) which is unsuitable for fast-CI test loops -- the
/// `set_timeout` synthetic outcome path lets the test exercise the same
/// `register-with-failed-resolve` shape without sleeping for the full
/// real timeout.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn kaspad_addpeer_hostname_resolve_timeout_metric() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    use kaspa_connectionmanager::test_support::FakeHostnameResolver;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_utils::networking::PeerEndpoint;

    let host = "timeout.kas947.invalid";
    let endpoint = PeerEndpoint::from_str(host).expect("parse hostname endpoint");
    let resolver = Arc::new(FakeHostnameResolver::new());
    let devnet_p2p_port = NetworkId::new(NetworkType::Devnet).default_p2p_port();
    resolver.set_timeout(host, devnet_p2p_port);

    let refresh_interval_sec = 2u64;
    let args = Args {
        devnet: true,
        disable_upnp: true,
        add_peers: vec![endpoint],
        hostname_refresh_interval_sec: refresh_interval_sec,
        ..Default::default()
    };
    let overrides = DaemonOverrides { hostname_resolver: Some(resolver.clone()) };
    let mut kaspad = Daemon::new_random_with_args_and_overrides(args, overrides, 10);
    // start() registers the timeout-resolving hostname endpoint. The
    // tolerate-unresolvable design returns a working RPC client even
    // though the resolver fires `PeerEndpointResolveError::Timeout`.
    let rpc_client = kaspad.start().await;

    // Liveness probe: a real timeout from the resolver is observable
    // only via metrics, never as a startup abort.
    assert!(
        rpc_client.handle_message_id(),
        "RPC client did not collect server features: kaspad must keep running across a Timeout-arm resolve",
    );

    // Initial registration counts toward the `initial` trigger bucket;
    // any subsequent retry under the `InitialRetry` cadence anchor
    // counts toward `initial_retry` instead. Asserting against
    // `initial_failed` ties the test to the Timeout-arm at registration
    // exclusively, independent of how many cadence ticks elapse.
    let snapshot =
        kaspad.hostname_metrics_snapshot().await.expect("connection manager should be wired into the flow context after start()");
    assert!(
        snapshot.resolutions_total.initial_failed >= 1,
        "expected initial_failed >= 1 after registering a timeout-only hostname; snapshot = {snapshot:?}; resolver call_count = {}",
        resolver.call_count(),
    );
    assert_eq!(
        snapshot.resolutions_total.initial_ok, 0,
        "no successful resolution expected on the Timeout-arm; snapshot = {snapshot:?}",
    );

    // Hold the observation window long enough for at least one
    // `InitialRetry` cadence tick after registration. The retry must
    // also deterministically produce a Timeout (resolver entry stays
    // installed), so `initial_retry_failed` is also expected to reach
    // 1. This second assertion ties the test to the cross-cadence
    // shape, not just the registration moment.
    tokio::time::sleep(Duration::from_secs(2 * refresh_interval_sec + 1)).await;
    let snapshot = kaspad
        .hostname_metrics_snapshot()
        .await
        .expect("connection manager should be wired into the flow context after observation window");
    assert!(
        snapshot.resolutions_total.initial_retry_failed >= 1,
        "expected initial_retry_failed >= 1 after at least one cadence tick on a Timeout-only hostname; snapshot = {snapshot:?}; resolver call_count = {}",
        resolver.call_count(),
    );

    rpc_client.disconnect().await.unwrap();
    drop(rpc_client);
    kaspad.shutdown();
}

/// `--addpeer=<host> --hostname-refresh-interval=2` produces at least two
/// `peer_hostname_resolutions_total{trigger="periodic"}` increments inside
/// a five-second observation window. The fake resolver pins `<host>` to a
/// loopback socket address so no real DNS is consulted, and the metric
/// counter is read directly off the running daemon's
/// [`kaspa_connectionmanager::ConnectionManager`] via the integration
/// harness's [`Daemon::hostname_metrics_snapshot`] hook.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn kaspad_periodic_refresh_observed() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    use kaspa_connectionmanager::test_support::FakeHostnameResolver;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_utils::networking::PeerEndpoint;
    use std::net::SocketAddr;

    let host = "fakehost.kas947.invalid";
    let endpoint = PeerEndpoint::from_str(host).expect("parse hostname endpoint");
    let resolver = Arc::new(FakeHostnameResolver::new());
    // The connection manager calls the resolver with the active network's
    // default p2p port when the `add_peers` endpoint omits an explicit
    // port. Derive the port from the `kaspa-consensus-core` `NetworkId`
    // API so a future consensus-side port change cannot silently regress
    // the resolver mapping to a stale literal.
    let devnet_p2p_port = NetworkId::new(NetworkType::Devnet).default_p2p_port();
    let stub_addr: SocketAddr = "127.0.0.1:42101".parse().unwrap();
    resolver.set(host, devnet_p2p_port, vec![stub_addr]);

    let args = Args {
        devnet: true,
        disable_upnp: true,
        add_peers: vec![endpoint],
        // Two-second cadence keeps the observation window short.
        hostname_refresh_interval_sec: 2,
        ..Default::default()
    };
    let overrides = DaemonOverrides { hostname_resolver: Some(resolver.clone()) };
    let mut kaspad = Daemon::new_random_with_args_and_overrides(args, overrides, 10);
    let rpc_client = kaspad.start().await;
    // Wait long enough for >=2 periodic ticks at the 2 s cadence even under
    // CI load (the ticker uses MissedTickBehavior::Delay; a single slipped
    // tick must not flake the assertion). 15 s gives ~7 ticks of headroom.
    tokio::time::sleep(Duration::from_secs(15)).await;
    let snapshot =
        kaspad.hostname_metrics_snapshot().await.expect("connection manager should be wired into the flow context after start()");
    assert!(
        snapshot.resolutions_total.periodic_ok >= 2,
        "expected periodic_ok >= 2 after 15s with 2s cadence; snapshot = {snapshot:?}; resolver call_count = {}",
        resolver.call_count(),
    );
    assert_eq!(snapshot.resolutions_total.initial_ok, 1, "exactly one initial resolution expected; snapshot = {snapshot:?}");
    rpc_client.disconnect().await.unwrap();
    drop(rpc_client);
    kaspad.shutdown();
}

/// A hostname-origin dial against a port that no peer is listening on
/// fails, marks the hostname entry stale, and triggers a re-resolution at
/// the next refresh tick. The fake resolver swaps its response between
/// the failing IP and a different IP after the dial-failure marker fires,
/// so the dial-failure-triggered re-resolve is observable both via the
/// metrics counter and via the resolver's invocation count.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn kaspad_dial_failure_re_resolves() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("INFO");

    use kaspa_connectionmanager::test_support::FakeHostnameResolver;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_utils::networking::PeerEndpoint;
    use std::net::SocketAddr;

    let host = "rotating.kas947.invalid";
    let endpoint = PeerEndpoint::from_str(host).unwrap();
    let resolver = Arc::new(FakeHostnameResolver::new());
    // Derive the active network's default p2p port from the
    // `kaspa-consensus-core` `NetworkId` API so the resolver mapping stays
    // in sync with whatever the connection manager hands it.
    let devnet_p2p_port = NetworkId::new(NetworkType::Devnet).default_p2p_port();
    // Initial response: a port that nothing is listening on. The dial
    // attempt against it will fail (connection refused), which the
    // connection manager translates into a hostname stale-mark.
    let ip_a: SocketAddr = "127.0.0.1:1".parse().unwrap();
    let ip_b: SocketAddr = "127.0.0.1:2".parse().unwrap();
    resolver.set(host, devnet_p2p_port, vec![ip_a]);

    let args = Args {
        devnet: true,
        disable_upnp: true,
        add_peers: vec![endpoint],
        // Short cadence so the dial-failure-triggered re-resolve is
        // observed inside the test deadline. The dial-failure path forces
        // an immediate re-resolve regardless of cadence; the cadence is
        // a safety net.
        hostname_refresh_interval_sec: 2,
        ..Default::default()
    };
    let overrides = DaemonOverrides { hostname_resolver: Some(resolver.clone()) };
    let mut kaspad = Daemon::new_random_with_args_and_overrides(args, overrides, 10);
    let rpc_client = kaspad.start().await;
    // After the daemon settles, swap the resolver to point at a different
    // socket so the next refresh observes a delta (and the dial-failure
    // path stamps `dial_failure_ok` when the re-resolve succeeds).
    tokio::time::sleep(Duration::from_secs(3)).await;
    resolver.set(host, devnet_p2p_port, vec![ip_b]);

    // Wait long enough to observe at least one dial-failure-triggered
    // re-resolution event. The dial loop ticks every 30 s by default, so a
    // 60 s polling budget absorbs the worst case (one full ticker period
    // landing just past the start of the window) plus CI-load overhead.
    let mut observed = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Some(snapshot) = kaspad.hostname_metrics_snapshot().await
            && snapshot.resolutions_total.dial_failure_ok >= 1
        {
            observed = true;
            break;
        }
    }
    let final_snapshot = kaspad.hostname_metrics_snapshot().await.unwrap_or_default();
    assert!(
        observed,
        "expected dial_failure_ok >= 1 within ~60s; final snapshot = {final_snapshot:?}; resolver call_count = {}",
        resolver.call_count(),
    );

    rpc_client.disconnect().await.unwrap();
    drop(rpc_client);
    kaspad.shutdown();
}

// =============================================================================
// DNS-volatility integration tests (silent-on-no-delta + toggle suites)
// =============================================================================

mod dns_volatility {
    use super::{Daemon, init_allocator_with_default_settings};
    use kaspa_connectionmanager::test_support::FakeHostnameResolver;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_utils::networking::PeerEndpoint;
    use kaspad_lib::{args::Args, daemon::DaemonOverrides};
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex as StdMutex};
    use std::time::Duration;

    /// In-memory `log::Log` shim. Tests install it via
    /// [`install_capturing_logger`] before [`Daemon::start`] so every line
    /// the running daemon emits via `info!` / `warn!` lands in a
    /// `Vec<String>` the test can scan for `addpeer:` substrings. The
    /// install replaces the standard `kaspa_core::log::try_init_logger`
    /// console appender; tests in this module MUST NOT also call
    /// `try_init_logger` (the global logger is single-set; second call
    /// would race the appender installed here).
    ///
    /// Per-test isolation relies on `cargo nextest` running each
    /// integration test in its own subprocess (the rusty-kaspa CI default
    /// per `scopes/rust.md`); the `set_boxed_logger` call panics on a
    /// double-install so a regression to a shared-process runner is
    /// surfaced loudly rather than silently passing on empty captures.
    struct CapturingLogger {
        lines: Arc<StdMutex<Vec<String>>>,
    }

    impl log::Log for CapturingLogger {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            let line = format!("{} {}: {}", record.level(), record.target(), record.args());
            // Mirror to stderr so a failing assertion still surfaces context
            // in the nextest captured-output panel.
            eprintln!("{line}");
            self.lines.lock().unwrap().push(line);
        }
        fn flush(&self) {}
    }

    fn install_capturing_logger() -> Arc<StdMutex<Vec<String>>> {
        let lines = Arc::new(StdMutex::new(Vec::new()));
        let logger = Box::new(CapturingLogger { lines: lines.clone() });
        log::set_boxed_logger(logger).expect(
            "global logger must be unset at install time; \
             cargo nextest runs each integration test in its own process",
        );
        log::set_max_level(log::LevelFilter::Info);
        lines
    }

    /// Snapshot of the captured log lines since process start. Cloning
    /// keeps subsequent comparisons lock-free.
    fn snapshot(lines: &Arc<StdMutex<Vec<String>>>) -> Vec<String> {
        lines.lock().unwrap().clone()
    }

    /// Count of `addpeer:` lines (any level) referencing `host`.
    /// Production logging vocabulary: registration uses `addpeer:`,
    /// reconciliation deltas use `addpeer:`, dial-loop logs use other
    /// prefixes (filtered out by the substring test).
    fn addpeer_count(lines: &[String], host: &str) -> usize {
        lines.iter().filter(|l| l.contains("addpeer:") && l.contains(host)).count()
    }

    /// Hold an arm for `cadence_sec` (first-tick window) plus
    /// `2 * cadence_sec` (intra-arm window), and return
    /// `(transition_delta, intra_arm_delta)` -- the count of new
    /// `addpeer:` lines for `host` produced inside each window.
    /// Caller asserts `transition_delta <= 1` (per-discipline
    /// one-shot allowed) and `intra_arm_delta == 0` (silence between
    /// transitions).
    async fn arm_observe(lines: &Arc<StdMutex<Vec<String>>>, host: &str, cadence_sec: u64) -> (usize, usize) {
        let pre_first = snapshot(lines);
        tokio::time::sleep(Duration::from_millis(cadence_sec * 1000 + 500)).await;
        let post_first = snapshot(lines);
        tokio::time::sleep(Duration::from_millis(2 * cadence_sec * 1000 + 500)).await;
        let post_intra = snapshot(lines);
        let transition_delta = addpeer_count(&post_first, host) - addpeer_count(&pre_first, host);
        let intra_delta = addpeer_count(&post_intra, host) - addpeer_count(&post_first, host);
        (transition_delta, intra_delta)
    }

    /// Single-arm DNS-failure suite. The fake resolver returns `Err` for
    /// the addpeer host across every periodic refresh tick the daemon
    /// fires inside the observation window. Locks two contracts:
    ///
    /// 1. **Silent-on-no-delta:** the daemon emits exactly one `addpeer:`
    ///    warn line for the host -- the registration-time one-shot from
    ///    `add_endpoint_request`. Subsequent failed refreshes do NOT
    ///    re-emit the warn (the discipline encoded in
    ///    `ConnectionManager::refresh_hostnames` Phase 4: `info!` only
    ///    fires on a non-empty delta; `apply_refresh_results` on `Err`
    ///    bumps `refresh_failures` silently).
    /// 2. **Periodic ticks actually fired:** the metric counter
    ///    `peer_hostname_resolutions_total` increments at least 4 times
    ///    on the failure path during the window, proving the silence is
    ///    real work-not-skipped. The label is `initial_retry_failed`
    ///    (NOT `periodic_failed` and NOT `dial_failure_failed`) because
    ///    the entry was registered with `StaleReason::InitialRetry`
    ///    (see `add_endpoint_request`) and never resolved successfully;
    ///    `pending_refreshes` derives the trigger from the per-entry
    ///    `stale_reason`, so an unresolvable host stays in the
    ///    `InitialRetry` bucket until DNS recovers.
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn kaspad_unresolvable_periodic_no_log_spam() {
        init_allocator_with_default_settings();
        let lines = install_capturing_logger();

        let host = "no-spam.kas947.invalid";
        let endpoint = PeerEndpoint::from_str(host).expect("parse hostname endpoint");
        let resolver = Arc::new(FakeHostnameResolver::new());
        let devnet_p2p_port = NetworkId::new(NetworkType::Devnet).default_p2p_port();
        resolver.set_err(host, devnet_p2p_port, "fake resolver: no-spam.kas947.invalid never resolves");

        let refresh_interval_sec = 1u64;
        let args = Args {
            devnet: true,
            disable_upnp: true,
            add_peers: vec![endpoint],
            hostname_refresh_interval_sec: refresh_interval_sec,
            ..Default::default()
        };
        let overrides = DaemonOverrides { hostname_resolver: Some(resolver.clone()) };
        let mut kaspad = Daemon::new_random_with_args_and_overrides(args, overrides, 10);
        let rpc_client = kaspad.start().await;

        // Observation window: cadence 1 s, sleep 6 s. The ticker skips
        // its immediate-fire tick, so the first periodic tick lands at
        // ~ t=1 s and at least 5 ticks fire by t=6 s. CI-jitter headroom
        // keeps the >= 4 metric assertion comfortable.
        tokio::time::sleep(Duration::from_secs(6)).await;

        let snap = snapshot(&lines);
        let metrics =
            kaspad.hostname_metrics_snapshot().await.expect("connection manager should be wired into the flow context after start()");

        let warn_lines: Vec<&String> =
            snap.iter().filter(|l| l.starts_with("WARN") && l.contains("addpeer:") && l.contains(host)).collect();
        assert_eq!(
            warn_lines.len(),
            1,
            "expected exactly one addpeer warn line for {host} (the registration one-shot); got {}: {warn_lines:?}",
            warn_lines.len(),
        );
        let info_lines: Vec<&String> =
            snap.iter().filter(|l| l.starts_with("INFO") && l.contains("addpeer:") && l.contains(host)).collect();
        assert!(info_lines.is_empty(), "expected zero addpeer info lines for {host} on the unresolvable path; got: {info_lines:?}",);

        // Cross-check: the periodic refresh task DID run (>= 4 failure
        // increments inside the 6 s window). The discipline labels these
        // increments `initial_retry_failed` -- the initial-failed register
        // marks the entry stale with `StaleReason::InitialRetry`, and
        // `pending_refreshes` derives the per-entry trigger from
        // `stale_reason` so an entry that has never resolved successfully
        // stays in the InitialRetry bucket until DNS recovers.
        assert!(
            metrics.resolutions_total.initial_failed >= 1,
            "expected initial_failed >= 1 after registering an unresolvable hostname; metrics = {metrics:?}; resolver call_count = {}",
            resolver.call_count(),
        );
        assert!(
            metrics.resolutions_total.initial_retry_failed >= 4,
            "expected initial_retry_failed >= 4 after 6 s @ 1 s cadence (proves periodic ticks fired); metrics = {metrics:?}; resolver call_count = {}",
            resolver.call_count(),
        );
        assert_eq!(
            metrics.resolutions_total.dial_failure_failed, 0,
            "dial_failure_failed must remain 0 for a never-resolved host (the dial loop never flagged this entry); metrics = {metrics:?}",
        );
        // The resolver was hit at least: 1 initial + 4 ticks = 5 calls.
        assert!(
            resolver.call_count() >= 5,
            "expected resolver call_count >= 5 (1 initial + >=4 periodic ticks); got {}",
            resolver.call_count(),
        );

        // Daemon stayed up across the window.
        assert!(rpc_client.handle_message_id(), "RPC client lost server liveness during the unresolvable-host observation window",);

        rpc_client.disconnect().await.unwrap();
        drop(rpc_client);
        kaspad.shutdown();
    }

    /// Toggle suite seeded UNRESOLVABLE. Cycles through arms
    /// `unresolvable -> resolvable -> unresolvable -> resolvable`, each
    /// arm holding for >= 2 periodic-refresh ticks. Locks the
    /// intra-arm-silence contract: across the K-1 ticks AFTER the first
    /// tick of any new arm, the daemon emits zero new `addpeer:` lines.
    /// The first tick of an arm may emit one transition log line per
    /// the current discipline (the seeded-unresolvable arm emits the
    /// initial registration warn; the first-resolvable arm emits a
    /// `+1 new` reconciliation info line; subsequent transitions are
    /// silent because `last_resolved` is preserved on `Err` and the
    /// resolvable arms reuse the same socket address).
    ///
    /// Cross-checks the metric interleave (`dial_failure_*` on the
    /// first transition out of the seeded-stale state, `periodic_*`
    /// from then on) and the `last_resolved` invariant via
    /// `HostnameMetricsSnapshot.resolved_addrs`: once a resolvable arm
    /// installs the socket address, the gauge stays at >= 1 across
    /// every subsequent unresolvable arm (the failure path never
    /// clears the registry).
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn kaspad_unresolvable_to_resolvable_toggle() {
        init_allocator_with_default_settings();
        let lines = install_capturing_logger();

        let host = "toggle-u2r.kas947.invalid";
        let endpoint = PeerEndpoint::from_str(host).expect("parse hostname endpoint");
        let resolver = Arc::new(FakeHostnameResolver::new());
        let devnet_p2p_port = NetworkId::new(NetworkType::Devnet).default_p2p_port();
        let stub: SocketAddr = "127.0.0.1:42201".parse().unwrap();
        // Seed: unresolvable.
        resolver.set_err(host, devnet_p2p_port, "fake resolver: toggle-u2r.kas947.invalid (initial unresolvable)");

        let refresh_interval_sec = 1u64;
        let args = Args {
            devnet: true,
            disable_upnp: true,
            add_peers: vec![endpoint],
            hostname_refresh_interval_sec: refresh_interval_sec,
            ..Default::default()
        };
        let overrides = DaemonOverrides { hostname_resolver: Some(resolver.clone()) };
        let mut kaspad = Daemon::new_random_with_args_and_overrides(args, overrides, 10);
        let rpc_client = kaspad.start().await;

        // Daemon::start() completes the initial registration before
        // returning. The registration discipline for the unresolvable
        // seed is exactly 1 warn line ("addpeer: ...; queued for
        // periodic retry"); lock that here, separately from the
        // per-arm windows below which only cover post-registration
        // ticks.
        let after_register = snapshot(&lines);
        let warn_after_register =
            after_register.iter().filter(|l| l.starts_with("WARN") && l.contains("addpeer:") && l.contains(host)).count();
        assert_eq!(
            warn_after_register, 1,
            "registration discipline (unresolvable seed): exactly 1 addpeer warn line; got {warn_after_register}; snapshot = {after_register:?}",
        );

        // Arm 1: seeded unresolvable, ticks all run after registration.
        // Failure-path ticks are silent.
        let (arm1_transition, arm1_intra) = arm_observe(&lines, host, refresh_interval_sec).await;
        assert_eq!(
            arm1_transition, 0,
            "arm 1 (post-register, unresolvable) tick window must be silent (failure path doesn't log); got {arm1_transition}",
        );
        assert_eq!(arm1_intra, 0, "arm 1 (unresolvable) intra-arm addpeer lines must be zero; got {arm1_intra}");

        // Switch to resolvable. Arm 2: first tick is labelled
        // InitialRetry (the registration set `stale_reason =
        // InitialRetry`; the entry has never resolved successfully).
        // The first successful resolve produces the +1-new delta info
        // line, clears `stale_reason`, and stamps `last_refresh`.
        // Subsequent ticks are silent (same socket address, empty
        // delta) and labelled Periodic.
        resolver.set(host, devnet_p2p_port, vec![stub]);
        let (arm2_transition, arm2_intra) = arm_observe(&lines, host, refresh_interval_sec).await;
        assert_eq!(
            arm2_transition, 1,
            "arm 2 (unresolvable -> resolvable) discipline produces exactly 1 transition addpeer line (+1 new reconciliation info); got {arm2_transition}",
        );
        assert_eq!(arm2_intra, 0, "arm 2 (resolvable) intra-arm addpeer lines must be zero; got {arm2_intra}");

        // Resolvable arm planted the socket: the registry gauge is now
        // >= 1; the invariant is that the failure path below preserves
        // it (entries are removed only via explicit mark_stale, never
        // by failure paths).
        let after_arm2 = kaspad.hostname_metrics_snapshot().await.expect("snapshot after resolvable arm");
        assert!(
            after_arm2.resolved_addrs >= 1,
            "after resolvable arm 2: resolved_addrs gauge must be >= 1; got {}",
            after_arm2.resolved_addrs,
        );

        // Switch back to unresolvable. Arm 3: ticks labelled Periodic
        // (last_refresh is now Some(...) from arm 2's success). Failure
        // path leaves last_resolved untouched -> gauge stays >= 1 and
        // no new log lines fire.
        resolver.set_err(host, devnet_p2p_port, "fake resolver: toggle-u2r.kas947.invalid (toggle to unresolvable)");
        let (arm3_transition, arm3_intra) = arm_observe(&lines, host, refresh_interval_sec).await;
        assert_eq!(
            arm3_transition, 0,
            "arm 3 (resolvable -> unresolvable) discipline produces zero transition addpeer lines (failure path is silent); got {arm3_transition}",
        );
        assert_eq!(arm3_intra, 0, "arm 3 (unresolvable) intra-arm addpeer lines must be zero; got {arm3_intra}");
        let after_arm3 = kaspad.hostname_metrics_snapshot().await.expect("snapshot after unresolvable arm");
        assert_eq!(
            after_arm3.resolved_addrs, after_arm2.resolved_addrs,
            "last_resolved invariant broken: failure path cleared the registry across arm 3 (was {} now {})",
            after_arm2.resolved_addrs, after_arm3.resolved_addrs,
        );

        // Switch to resolvable with the SAME socket. Arm 4: ticks
        // resolve OK; delta is empty (last_resolved unchanged); no log
        // line at all.
        resolver.set(host, devnet_p2p_port, vec![stub]);
        let (arm4_transition, arm4_intra) = arm_observe(&lines, host, refresh_interval_sec).await;
        assert_eq!(
            arm4_transition, 0,
            "arm 4 (resolvable, same IP) transition addpeer lines must be zero (delta empty); got {arm4_transition}",
        );
        assert_eq!(arm4_intra, 0, "arm 4 (resolvable) intra-arm addpeer lines must be zero; got {arm4_intra}");

        // Final metric interleave check: every counter we expected to
        // increment did, and the unresolvable seed bumps the
        // `initial_retry_*` buckets while the toggled arms move into
        // `periodic_*` once the entry has resolved successfully at
        // least once and `stale_reason` has been cleared.
        let final_metrics = kaspad.hostname_metrics_snapshot().await.expect("final snapshot");
        assert!(final_metrics.resolutions_total.initial_failed >= 1, "initial register failed at least once: {final_metrics:?}");
        assert!(
            final_metrics.resolutions_total.initial_retry_failed >= 1,
            "arm 1 (post-register, unresolvable) ticks must increment initial_retry_failed (entry never resolved yet): {final_metrics:?}",
        );
        assert!(
            final_metrics.resolutions_total.initial_retry_ok >= 1,
            "arm 2 first tick must increment initial_retry_ok (the entry has not resolved successfully yet at that point): {final_metrics:?}",
        );
        assert!(
            final_metrics.resolutions_total.periodic_failed >= 1,
            "arm 3 unresolvable ticks must increment periodic_failed (last_refresh advanced by arm 2): {final_metrics:?}",
        );
        assert!(
            final_metrics.resolutions_total.periodic_ok >= 1,
            "arm 4 resolvable ticks must increment periodic_ok: {final_metrics:?}",
        );
        assert_eq!(
            final_metrics.resolutions_total.dial_failure_failed, 0,
            "dial_failure_* buckets must remain at 0 in this test (no dial-loop interaction triggers DialFailure mark_stale): {final_metrics:?}",
        );

        assert!(rpc_client.handle_message_id(), "RPC client lost server liveness during the toggle window");
        rpc_client.disconnect().await.unwrap();
        drop(rpc_client);
        kaspad.shutdown();
    }

    /// Toggle suite seeded RESOLVABLE. Cycles through arms
    /// `resolvable -> unresolvable -> resolvable -> unresolvable`, each
    /// arm holding for >= 2 periodic-refresh ticks. Locks the same
    /// intra-arm-silence contract as the sibling test, exercised from
    /// the opposite phase: the seeded-resolvable arm produces the
    /// initial registration info line ("addpeer: resolved <host> ->
    /// [...]") and every subsequent arm is silent because (a)
    /// resolvable arms reuse the same socket so deltas are empty, and
    /// (b) failure arms preserve `last_resolved` unchanged.
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn kaspad_resolvable_to_unresolvable_toggle() {
        init_allocator_with_default_settings();
        let lines = install_capturing_logger();

        let host = "toggle-r2u.kas947.invalid";
        let endpoint = PeerEndpoint::from_str(host).expect("parse hostname endpoint");
        let resolver = Arc::new(FakeHostnameResolver::new());
        let devnet_p2p_port = NetworkId::new(NetworkType::Devnet).default_p2p_port();
        let stub: SocketAddr = "127.0.0.1:42301".parse().unwrap();
        // Seed: resolvable.
        resolver.set(host, devnet_p2p_port, vec![stub]);

        let refresh_interval_sec = 1u64;
        let args = Args {
            devnet: true,
            disable_upnp: true,
            add_peers: vec![endpoint],
            hostname_refresh_interval_sec: refresh_interval_sec,
            ..Default::default()
        };
        let overrides = DaemonOverrides { hostname_resolver: Some(resolver.clone()) };
        let mut kaspad = Daemon::new_random_with_args_and_overrides(args, overrides, 10);
        let rpc_client = kaspad.start().await;

        // Daemon::start() completes the initial registration before
        // returning. The registration discipline for the resolvable
        // seed is exactly 1 info line ("addpeer: resolved ..."); lock
        // that here, separately from the per-arm windows below which
        // only cover post-registration ticks.
        let after_register = snapshot(&lines);
        let info_after_register =
            after_register.iter().filter(|l| l.starts_with("INFO") && l.contains("addpeer:") && l.contains(host)).count();
        assert_eq!(
            info_after_register, 1,
            "registration discipline (resolvable seed): exactly 1 addpeer info line; got {info_after_register}; snapshot = {after_register:?}",
        );

        // Arm 1: seeded resolvable, ticks all run after registration.
        // trigger=Periodic, same IP -> delta empty -> silent.
        let (arm1_transition, arm1_intra) = arm_observe(&lines, host, refresh_interval_sec).await;
        assert_eq!(
            arm1_transition, 0,
            "arm 1 (post-register, resolvable, same IP) tick window must be silent (delta empty); got {arm1_transition}",
        );
        assert_eq!(arm1_intra, 0, "arm 1 (resolvable) intra-arm addpeer lines must be zero; got {arm1_intra}");

        let after_arm1 = kaspad.hostname_metrics_snapshot().await.expect("snapshot after resolvable arm");
        assert!(
            after_arm1.resolved_addrs >= 1,
            "after resolvable arm 1: resolved_addrs gauge must be >= 1; got {}",
            after_arm1.resolved_addrs,
        );

        // Switch to unresolvable. Arm 2: ticks Periodic_failed, silent,
        // last_resolved preserved.
        resolver.set_err(host, devnet_p2p_port, "fake resolver: toggle-r2u.kas947.invalid (toggle to unresolvable)");
        let (arm2_transition, arm2_intra) = arm_observe(&lines, host, refresh_interval_sec).await;
        assert_eq!(arm2_transition, 0, "arm 2 (unresolvable) transition addpeer lines must be zero; got {arm2_transition}");
        assert_eq!(arm2_intra, 0, "arm 2 (unresolvable) intra-arm addpeer lines must be zero; got {arm2_intra}");
        let after_arm2 = kaspad.hostname_metrics_snapshot().await.expect("snapshot after unresolvable arm");
        assert_eq!(
            after_arm2.resolved_addrs, after_arm1.resolved_addrs,
            "last_resolved invariant broken: failure path cleared the registry across arm 2 (was {} now {})",
            after_arm1.resolved_addrs, after_arm2.resolved_addrs,
        );

        // Switch back to resolvable with the SAME socket. Arm 3: ticks
        // Periodic_ok, delta empty, silent.
        resolver.set(host, devnet_p2p_port, vec![stub]);
        let (arm3_transition, arm3_intra) = arm_observe(&lines, host, refresh_interval_sec).await;
        assert_eq!(arm3_transition, 0, "arm 3 (resolvable, same IP) transition addpeer lines must be zero; got {arm3_transition}");
        assert_eq!(arm3_intra, 0, "arm 3 (resolvable) intra-arm addpeer lines must be zero; got {arm3_intra}");
        let after_arm3 = kaspad.hostname_metrics_snapshot().await.expect("snapshot after resolvable arm 3");
        assert_eq!(
            after_arm3.resolved_addrs, after_arm2.resolved_addrs,
            "resolved_addrs unchanged across resolvable arm 3 (same IP): was {} now {}",
            after_arm2.resolved_addrs, after_arm3.resolved_addrs,
        );

        // Switch back to unresolvable. Arm 4: ticks Periodic_failed,
        // silent, last_resolved preserved.
        resolver.set_err(host, devnet_p2p_port, "fake resolver: toggle-r2u.kas947.invalid (final unresolvable)");
        let (arm4_transition, arm4_intra) = arm_observe(&lines, host, refresh_interval_sec).await;
        assert_eq!(arm4_transition, 0, "arm 4 (unresolvable) transition addpeer lines must be zero; got {arm4_transition}");
        assert_eq!(arm4_intra, 0, "arm 4 (unresolvable) intra-arm addpeer lines must be zero; got {arm4_intra}");
        let after_arm4 = kaspad.hostname_metrics_snapshot().await.expect("final snapshot");
        assert_eq!(
            after_arm4.resolved_addrs, after_arm3.resolved_addrs,
            "last_resolved invariant broken: failure path cleared the registry across arm 4 (was {} now {})",
            after_arm3.resolved_addrs, after_arm4.resolved_addrs,
        );

        // Final metric interleave: initial registration succeeded, the
        // failure arms moved Periodic_failed, the resolvable arms moved
        // Periodic_ok. dial_failure_* should remain at 0 (the entry was
        // never marked stale -- arm 1 succeeded so last_refresh is
        // never None inside this test).
        assert!(after_arm4.resolutions_total.initial_ok >= 1, "initial resolvable register: {after_arm4:?}");
        assert!(
            after_arm4.resolutions_total.periodic_failed >= 2,
            "arms 2+4 unresolvable must increment periodic_failed; metrics = {after_arm4:?}",
        );
        assert!(
            after_arm4.resolutions_total.periodic_ok >= 2,
            "arms 1+3 resolvable must increment periodic_ok; metrics = {after_arm4:?}",
        );
        assert_eq!(
            after_arm4.resolutions_total.dial_failure_failed, 0,
            "no dial-failure-triggered re-resolution should fire in this test (no dial loop interaction); got {after_arm4:?}",
        );

        assert!(rpc_client.handle_message_id(), "RPC client lost server liveness during the toggle window");
        rpc_client.disconnect().await.unwrap();
        drop(rpc_client);
        kaspad.shutdown();
    }
}
