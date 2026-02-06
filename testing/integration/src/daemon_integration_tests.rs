use crate::common::{
    client::ListeningClient,
    client_notify::ChannelNotify,
    daemon::Daemon,
    utils::{fetch_spendable_utxos, generate_tx, mine_block, required_fee, wait_for},
};
use kaspa_addresses::Address;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::params::{Params, SIMNET_GENESIS, SIMNET_PARAMS};
use kaspa_consensus_core::{
    config::params::OverrideParams,
    constants::TX_VERSION,
    header::Header,
    sign::sign,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{task::runtime::AsyncRuntime, trace};
use kaspa_grpc_client::GrpcClient;
use kaspa_hashes::Hash;
use kaspa_notify::scope::{BlockAddedScope, UtxosChangedScope, VirtualDaaScoreChangedScope};
use kaspa_rpc_core::{Notification, RpcTransactionId, api::rpc::RpcApi};
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
    let miner_schnorr_key = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &miner_sk);
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
    let transaction = generate_tx(miner_schnorr_key, &utxos[0..NUMBER_INPUTS as usize], TX_AMOUNT, NUMBER_OUTPUTS, &user_address);
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

    // Choose a target almost a full finality depth below the current tip.
    let dag_info = rpc_client1.get_block_dag_info().await.unwrap();
    let remaining = finality_depth.saturating_sub(1);
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

    // Spend the P2SH output to trigger seqcommit validation on the syncee.
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

    let template = rpc_client1.get_block_template(miner_address, vec![]).await.unwrap();
    rpc_client1.submit_block(template.block, false).await.unwrap();

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
