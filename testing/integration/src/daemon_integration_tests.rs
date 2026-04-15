use crate::common::{
    client::ListeningClient,
    client_notify::ChannelNotify,
    daemon::Daemon,
    utils::{fetch_spendable_utxos, mine_block, required_fee, wait_for},
};
use kaspa_addresses::Address;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::params::{Params, SIMNET_GENESIS, SIMNET_PARAMS};
use kaspa_consensus_core::{
    config::params::OverrideParams,
    constants::{TX_VERSION, TX_VERSION_POST_COV_HF},
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
use kaspa_notify::scope::{BlockAddedScope, UtxosChangedScope, VirtualDaaScoreChangedScope};
use kaspa_rpc_core::{Notification, RpcTransaction, RpcTransactionId, api::rpc::RpcApi};
use kaspa_txscript::{
    opcodes::codes, pay_to_address_script, pay_to_script_hash_script, pay_to_script_hash_signature_script,
    script_builder::ScriptBuilder,
};
use kaspad_lib::args::Args;
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

    // Create a multi-listener RPC client on each node...
    let mut clients = vec![ListeningClient::connect(&kaspad2).await, ListeningClient::connect(&kaspad1).await];

    // ...and subscribe each to some notifications
    for x in clients.iter_mut() {
        x.start_notify(BlockAddedScope {}.into()).await.unwrap();
        x.start_notify(UtxosChangedScope::new(vec![miner_address.clone(), user_address.clone()]).into()).await.unwrap();
        x.start_notify(VirtualDaaScoreChangedScope {}.into()).await.unwrap();
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
            mass: ComputeBudget(0).into(),
        })
        .collect();
    let outputs = (0..NUMBER_OUTPUTS)
        .map(|_| TransactionOutput {
            value: TX_AMOUNT / NUMBER_OUTPUTS,
            script_public_key: tx_script_public_key.clone(),
            covenant: None,
        })
        .collect();
    let unsigned_tx = Transaction::new(TX_VERSION_POST_COV_HF, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signed_tx = sign_with_multiple_v2(
        MutableTransaction::with_entries(unsigned_tx, selected_utxos.iter().map(|(_, entry)| entry.clone()).collect()),
        &[miner_sk.secret_bytes()],
    )
    .unwrap();
    let mut transaction = signed_tx.tx;
    let per_input_compute_budget_commitment: u16 = 300; // ~30k-gram per-input upper bound
    transaction.inputs.iter_mut().for_each(|input| input.mass = ComputeBudget(per_input_compute_budget_commitment).into());
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
        testnet: true,
        testnet_suffix: 12,
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
    const EXTRA_FEE: u64 = 10_000;
    let oldest_utxos_start = utxos.len() - NUMBER_INPUTS as usize;
    let selected_utxos = &utxos[oldest_utxos_start..];
    let total_in = selected_utxos.iter().map(|x| x.1.amount).sum::<u64>();
    let tx_fee = required_fee(selected_utxos.len(), NUMBER_OUTPUTS).saturating_add(EXTRA_FEE);
    let tx_amount = total_in.checked_sub(tx_fee).expect("expected enough input value for test transaction fee");
    let script_public_key = pay_to_address_script(&user_address);
    let inputs = selected_utxos
        .iter()
        .map(|(op, _)| TransactionInput {
            previous_outpoint: *op,
            signature_script: vec![],
            sequence: 0,
            mass: ComputeBudget(0).into(),
        })
        .collect();
    let outputs = (0..NUMBER_OUTPUTS)
        .map(|_| TransactionOutput { value: tx_amount / NUMBER_OUTPUTS, script_public_key: script_public_key.clone(), covenant: None })
        .collect();
    let unsigned_tx = Transaction::new(TX_VERSION_POST_COV_HF, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signed_tx = sign_with_multiple_v2(
        MutableTransaction::with_entries(unsigned_tx, selected_utxos.iter().map(|(_, entry)| entry.clone()).collect()),
        &[miner_sk.secret_bytes()],
    )
    .unwrap();
    let mut transaction = signed_tx.tx;
    transaction.inputs.iter_mut().for_each(|input| input.mass = ComputeBudget(30).into());
    assert!(
        transaction.inputs.iter().any(|input| input.mass.compute_budget().unwrap() > 0),
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
    assert_eq!(node1_entry.transaction.version, TX_VERSION_POST_COV_HF);
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

    assert_eq!(included_tx.version, TX_VERSION_POST_COV_HF);
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
        testnet: true,
        testnet_suffix: 12,
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
        let fee = required_fee(1, 1);
        let output_value = selected_utxo.1.amount.checked_sub(fee).expect("expected enough input value for test fee");
        let mass = ComputeBudget(0).into(); // set correctly by sign below
        let tx = Transaction::new(
            version,
            vec![TransactionInput { previous_outpoint: selected_utxo.0, signature_script: vec![], sequence: 0, mass }],
            vec![TransactionOutput { value: output_value, script_public_key: pay_spk.clone(), covenant: None }],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        sign(MutableTransaction::with_entries(tx, vec![selected_utxo.1.clone()]), miner_schnorr_key).tx
    };

    let v1_tx = build_single_input_tx(TX_VERSION_POST_COV_HF, &utxos[0]);
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
    // almost a full finality depth below the tip.
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

    // Choose a target almost a full finality depth below the current tip, leaving
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
    let fee = required_fee(input_utxos.len(), 1);
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
    let spend_fee = required_fee(1, 1);
    let spend_value = total_in - fee - spend_fee;
    let signature_script = pay_to_script_hash_signature_script(redeem_script, vec![]).expect("canonical signature script");
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
    let mut dag_info = rpc_client1.get_block_dag_info().await.unwrap();
    let mut extra_blocks = 0usize;
    let extra_blocks_limit = params.pruning_depth().saturating_add(30) as usize;
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
        // `rest != ZEROES` is the post-HF non-reserved subnetwork rule
        // (consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs:165).
        // Putting a distinct nonzero byte in position 19 satisfies it while keeping
        // each lane_id unique.
        let mut subnet_bytes = [0u8; 20];
        subnet_bytes[19] = (i as u8) + 1;
        let lane_subnet = SubnetworkId::from_bytes(subnet_bytes);

        let fee = required_fee(1, 1);
        assert!(entry.amount > fee, "coinbase utxo is too small to cover a tx fee");
        let out_value = entry.amount - fee;
        let unsigned_tx = Transaction::new(
            TX_VERSION_POST_COV_HF,
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
