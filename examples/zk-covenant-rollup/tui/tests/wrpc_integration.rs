//! Integration tests for the TUI's wRPC client layer.
//!
//! These tests spin up real Kaspa daemons (simnet) and exercise the
//! `KaspaNode` wrapper and related functionality over wRPC.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::network::NetworkType;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_testing_integration::common::daemon::Daemon;
use kaspa_wrpc_client::prelude::*;
use kaspad_lib::args::Args;

use zk_covenant_rollup_tui::app::{ActionType, App, InputMode, Tab};
use zk_covenant_rollup_tui::db::RollupDb;
use zk_covenant_rollup_tui::node::KaspaNode;

fn simnet_args() -> Args {
    Args { simnet: true, unsafe_rpc: true, enable_unsynced_mining: true, utxoindex: true, disable_upnp: true, ..Default::default() }
}

fn zero_maturity_args() -> Args {
    let params_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/zero_maturity_params.json");
    Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        utxoindex: true,
        disable_upnp: true,
        override_params_file: Some(params_path.to_string_lossy().to_string()),
        ..Default::default()
    }
}

fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Create an App instance connected to a test daemon.
async fn setup_test_app(kaspad: &mut Daemon) -> (App, kaspa_grpc_client::GrpcClient, Address, secp256k1::Keypair) {
    let grpc_client = kaspad.start().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let url = format!("ws://localhost:{}", kaspad.rpc_borsh_port);
    let network_id = NetworkId::new(NetworkType::Simnet);
    let node = KaspaNode::try_new(&url, network_id).unwrap();
    node.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let dag_info = node.get_block_dag_info().await.unwrap();
    let tmpdir = tempfile::tempdir().unwrap();
    let db = Arc::new(RollupDb::open(tmpdir.path()).unwrap());
    let mut app = App::new(db, node, Prefix::Simnet);
    app.pruning_point = dag_info.pruning_point_hash;
    app.connected = true;

    let keypair = secp256k1::Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
    let address = Address::new(Prefix::Simnet, Version::PubKey, &keypair.x_only_public_key().0.serialize());

    (app, grpc_client, address, keypair)
}

/// Mine blocks via gRPC and wait for propagation.
async fn mine_blocks(grpc_client: &kaspa_grpc_client::GrpcClient, address: &Address, count: u64) {
    for _ in 0..count {
        let template = grpc_client.get_block_template(address.clone(), vec![]).await.unwrap();
        grpc_client.submit_block(template.block, false).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ── wRPC client tests (existing) ──

/// Test that a raw wRPC client can connect to the daemon and make basic RPC calls.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn wrpc_client_basic_rpc() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let _grpc_client = kaspad.start().await;

    // Wait for wRPC server
    tokio::time::sleep(Duration::from_millis(500)).await;

    let wrpc_client = kaspad.new_wrpc_client();
    wrpc_client.connect(None).await.expect("wRPC connect");

    // get_server_info
    let info = wrpc_client.get_server_info().await.expect("get_server_info");
    assert!(!info.server_version.is_empty());

    // get_block_dag_info
    let dag_info = wrpc_client.get_block_dag_info().await.expect("get_block_dag_info");
    assert_ne!(dag_info.pruning_point_hash, kaspa_hashes::Hash::default());

    wrpc_client.disconnect().await.unwrap();
    drop(wrpc_client);
    kaspad.shutdown();
}

/// Test that the TUI's `KaspaNode` wrapper can connect, query DAG info,
/// and receive events.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn kaspa_node_wrapper_connect() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let _grpc_client = kaspad.start().await;

    // Wait for wRPC server
    tokio::time::sleep(Duration::from_millis(500)).await;

    let url = format!("ws://localhost:{}", kaspad.rpc_borsh_port);
    let network_id = NetworkId::new(NetworkType::Simnet);

    let node = KaspaNode::try_new(&url, network_id).expect("try_new");
    node.connect().await.expect("connect");

    // Wait for the event task to process the Connected event
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(node.is_connected());

    // Query DAG info through the wrapper
    let dag_info = node.get_block_dag_info().await.expect("get_block_dag_info");
    assert_ne!(dag_info.pruning_point_hash, kaspa_hashes::Hash::default());

    // Stop
    node.stop().await.expect("stop");
    kaspad.shutdown();
}

/// Test mining blocks and receiving UTXO notifications via wRPC.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn wrpc_utxo_subscription() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let grpc_client = kaspad.start().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Set up the TUI node wrapper
    let url = format!("ws://localhost:{}", kaspad.rpc_borsh_port);
    let network_id = NetworkId::new(NetworkType::Simnet);
    let node = KaspaNode::try_new(&url, network_id).expect("try_new");
    node.connect().await.expect("connect");

    // Generate a keypair for mining
    let keypair = secp256k1::Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
    let pubkey = keypair.x_only_public_key().0;
    let address = Address::new(kaspa_addresses::Prefix::Simnet, kaspa_addresses::Version::PubKey, pubkey.serialize().as_slice());

    // Subscribe to UTXO changes for the miner address
    node.subscribe_utxos(vec![address.clone()]).await.expect("subscribe_utxos");

    // Mine some blocks via gRPC
    for _ in 0..5 {
        let template = grpc_client.get_block_template(address.clone(), vec![]).await.unwrap();
        grpc_client.submit_block(template.block, false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Wait for notifications to propagate
    tokio::time::sleep(Duration::from_secs(1)).await;

    // We should have received at least some events (DAA score changes, possibly UTXO changes)
    let event_rx = node.event_receiver();
    let mut event_count = 0;
    while let Ok(event) = event_rx.try_recv() {
        if let zk_covenant_rollup_tui::node::NodeEvent::Notification(_) = event {
            event_count += 1;
        }
    }
    assert!(event_count > 0, "Should have received at least one notification event");

    // Check UTXOs via wRPC
    let utxos = node.get_utxos_by_addresses(vec![address]).await.expect("get_utxos");
    assert!(!utxos.is_empty(), "Miner should have received coinbase UTXOs");

    node.stop().await.expect("stop");
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test fetching virtual chain data (VCCv2) through the wRPC client.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn wrpc_virtual_chain_v2() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let grpc_client = kaspad.start().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let url = format!("ws://localhost:{}", kaspad.rpc_borsh_port);
    let network_id = NetworkId::new(NetworkType::Simnet);
    let node = KaspaNode::try_new(&url, network_id).expect("try_new");
    node.connect().await.expect("connect");

    // Get initial chain state
    let dag_info = node.get_block_dag_info().await.expect("dag info");
    let pruning_point = dag_info.pruning_point_hash;

    // Mine some blocks
    let keypair = secp256k1::Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
    let pubkey = keypair.x_only_public_key().0;
    let address = Address::new(kaspa_addresses::Prefix::Simnet, kaspa_addresses::Version::PubKey, pubkey.serialize().as_slice());

    for _ in 0..10 {
        let template = grpc_client.get_block_template(address.clone(), vec![]).await.unwrap();
        grpc_client.submit_block(template.block, false).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Query VCCv2 from pruning point with no min confirmations
    let vcc = node.get_virtual_chain_v2(pruning_point, None).await.expect("get_virtual_chain_v2");

    assert!(!vcc.added_chain_block_hashes.is_empty(), "Should have added chain blocks after mining");
    assert!(!vcc.chain_block_accepted_transactions.is_empty(), "Should have accepted transactions (at least coinbase)");

    // Each block should have at least one accepted transaction (the coinbase)
    for block_txs in vcc.chain_block_accepted_transactions.iter() {
        assert!(!block_txs.accepted_transactions.is_empty(), "Each block should have at least coinbase");
    }

    node.stop().await.expect("stop");
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test the generic `fetch_spendable_utxos` utility works with the wRPC client.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn wrpc_fetch_spendable_utxos() {
    use kaspa_consensus::params::SIMNET_PARAMS;
    use kaspa_testing_integration::common::utils::fetch_spendable_utxos;

    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let grpc_client = kaspad.start().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Connect wRPC client
    let wrpc_client = kaspad.new_wrpc_client();
    wrpc_client.connect(None).await.expect("wRPC connect");

    let keypair = secp256k1::Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
    let pubkey = keypair.x_only_public_key().0;
    let address = Address::new(kaspa_addresses::Prefix::Simnet, kaspa_addresses::Version::PubKey, pubkey.serialize().as_slice());

    let coinbase_maturity = SIMNET_PARAMS.coinbase_maturity();

    // Mine enough blocks for coinbase to mature
    for _ in 0..(coinbase_maturity + 10) {
        let template = grpc_client.get_block_template(address.clone(), vec![]).await.unwrap();
        grpc_client.submit_block(template.block, false).await.unwrap();
        // No sleep needed — simnet has instant block times
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Use the generic fetch_spendable_utxos with the wRPC client
    let utxos = fetch_spendable_utxos(&wrpc_client, address.clone(), coinbase_maturity).await;
    assert!(!utxos.is_empty(), "Should have spendable UTXOs after mining past maturity");

    // Verify amounts are positive
    for (_, entry) in &utxos {
        assert!(entry.amount > 0);
    }

    wrpc_client.disconnect().await.unwrap();
    drop(wrpc_client);
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

// ── TUI App integration tests ──

/// Test creating and selecting a covenant.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_create_and_select_covenant() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, _address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create covenant via key handler (simulates pressing 'c' on Covenants tab)
    app.active_tab = Tab::Covenants;
    app.handle_key(key_event(KeyCode::Char('c')));

    assert_eq!(app.covenants.len(), 1, "Should have one covenant after creation");

    // Select it: set cursor to 0, press Enter
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));

    assert_eq!(app.selected_covenant, Some(0), "Covenant should be selected");
    // Not deployed yet, so prover should NOT be initialized
    assert!(app.prover.is_none(), "Prover should not init for undeployed covenant");

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test deploying a covenant.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_deploy_covenant() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, miner_address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create and select covenant
    app.active_tab = Tab::Covenants;
    app.handle_key(key_event(KeyCode::Char('c')));
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));

    // Get deployer address
    let deployer_addr_str = app.deployer_address(&app.covenants[0].1).unwrap();
    let deployer_addr: Address = deployer_addr_str.clone().try_into().unwrap();

    // Fund deployer with 100 coinbase UTxOs (at DAA ~1-100), then advance DAA 1100 more
    // blocks to a different address so ALL deployer coinbases satisfy the simnet coinbase
    // maturity of 1000 (current DAA ~1200 >= 100 + 1000).
    mine_blocks(&grpc_client, &deployer_addr, 100).await;
    mine_blocks(&grpc_client, &miner_address, 1100).await;

    // Subscribe and fetch UTXOs for the deployer
    app.subscribe_covenant_addresses();
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Verify deployer has funds
    let balance = app.utxo_tracker.balance(&deployer_addr_str);
    assert!(balance > 0, "Deployer should have funds, got balance: {balance}");

    // Deploy (simulates pressing 'd')
    app.handle_key(key_event(KeyCode::Char('d')));

    // Process the SubmitTransaction pending op
    app.process_pending_ops().await;

    // Verify deployment was recorded
    let cov = app.db.get_covenant(app.covenants[0].0).unwrap().unwrap();
    assert!(cov.deployment_tx_id.is_some(), "Deployment tx ID should be recorded in DB");

    // Verify tx appears in tx_history
    assert!(!app.tx_history.is_empty(), "Should have at least one tx in history");
    assert_eq!(app.tx_history[0].action, "Deploy");

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test continuous state sync (auto-prover init on deployed covenant select).
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_continuous_state_sync() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, miner_address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create covenant
    app.handle_key(key_event(KeyCode::Char('c')));

    // Get deployer address and fund it.
    // Mine 100 blocks to deployer (coinbases at DAA 1-100), then mine 1100 more blocks to miner
    // to advance DAA to ~1200 so ALL deployer coinbases satisfy simnet maturity of 1000.
    let deployer_addr_str = app.deployer_address(&app.covenants[0].1).unwrap();
    let deployer_addr: Address = deployer_addr_str.clone().try_into().unwrap();
    mine_blocks(&grpc_client, &deployer_addr, 100).await;
    mine_blocks(&grpc_client, &miner_address, 1100).await;

    // Select, subscribe, fetch UTXOs
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Deploy
    app.active_tab = Tab::Covenants;
    app.handle_key(key_event(KeyCode::Char('d')));
    app.process_pending_ops().await;

    // Refresh covenants list (deployment updated the DB)
    app.covenants = app.db.list_covenants();

    // Now re-select the covenant (now deployed) — should auto-init prover
    app.prover = None; // Reset prover
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));

    assert!(app.prover.is_some(), "Prover should auto-init for deployed covenant");

    // Process FetchAndProcessChain
    // Mine more blocks so there's something to process
    mine_blocks(&grpc_client, &miner_address, 5).await;
    app.process_pending_ops().await;

    // The prover should have processed some blocks
    let prover = app.prover.as_ref().unwrap();
    assert_ne!(prover.last_processed_block, app.pruning_point, "Prover should have advanced past pruning point");

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test the entry action input flow (prompt → confirm → processing).
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_entry_action_flow() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, _miner_address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create covenant, account
    app.handle_key(key_event(KeyCode::Char('c')));
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));
    app.active_tab = Tab::Accounts;
    app.handle_key(key_event(KeyCode::Char('c')));

    // Fund the account address
    let (pk, _) = app.accounts[0];
    let acct_addr = Address::new(Prefix::Simnet, Version::PubKey, &pk.as_bytes());
    mine_blocks(&grpc_client, &acct_addr, 120).await;

    // Subscribe & fetch UTXOs
    app.subscribe_covenant_addresses();
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let acct_addr_str = app.pubkey_to_address(&pk).unwrap();
    let balance = app.utxo_tracker.balance(&acct_addr_str);
    assert!(balance > 0, "Account should have funds");

    // Start entry action input
    app.active_tab = Tab::Actions;
    app.start_action_input(ActionType::Entry);
    assert!(matches!(app.input_mode, InputMode::PromptAmount { .. }), "Should be in PromptAmount mode");

    // Type "1000" (digit by digit)
    app.handle_input_key(key_event(KeyCode::Char('1')));
    app.handle_input_key(key_event(KeyCode::Char('0')));
    app.handle_input_key(key_event(KeyCode::Char('0')));
    app.handle_input_key(key_event(KeyCode::Char('0')));

    // Press Enter to confirm
    app.handle_input_key(key_event(KeyCode::Enter));
    assert!(matches!(app.input_mode, InputMode::Confirm { .. }), "Should be in Confirm mode");

    // Press Enter to submit
    app.handle_input_key(key_event(KeyCode::Enter));
    assert!(matches!(app.input_mode, InputMode::Processing { .. }), "Should be in Processing mode");

    // Process the BuildAndSubmitAction pending op
    app.process_pending_ops().await;

    // Should be back to Normal and have a tx in history
    assert!(app.input_mode.is_normal(), "Should be back to Normal mode");
    assert!(!app.tx_history.is_empty(), "Should have tx in history");
    assert_eq!(app.tx_history.last().unwrap().action, "Entry (Deposit)");

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test deleting an undeployed covenant (with confirmation popup).
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_delete_undeployed_covenant() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, _address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create covenant
    app.active_tab = Tab::Covenants;
    app.handle_key(key_event(KeyCode::Char('c')));
    assert_eq!(app.covenants.len(), 1);

    // Press 'x' — opens confirmation popup
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Char('x')));
    assert!(matches!(app.input_mode, InputMode::ConfirmDelete { .. }), "Should show ConfirmDelete popup");
    assert_eq!(app.covenants.len(), 1, "Not deleted yet — waiting for confirmation");

    // Press 'y' to confirm
    app.handle_input_key(key_event(KeyCode::Char('y')));
    assert!(app.input_mode.is_normal(), "Should return to Normal mode");
    assert!(app.covenants.is_empty(), "Covenant list should be empty after confirmed deletion");

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test deleting a deployed covenant: confirmation popup with WARNING, Esc cancels, 'y' deletes.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_delete_deployed_covenant_confirm() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, miner_address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create and select covenant
    app.handle_key(key_event(KeyCode::Char('c')));
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));

    // Fund deployer — mine 100 blocks to deployer (coinbases at DAA 1-100), then 1100 to miner
    // to advance DAA to ~1200 so ALL deployer coinbases satisfy simnet maturity of 1000.
    let deployer_addr_str = app.deployer_address(&app.covenants[0].1).unwrap();
    let deployer_addr: Address = deployer_addr_str.clone().try_into().unwrap();
    mine_blocks(&grpc_client, &deployer_addr, 100).await;
    mine_blocks(&grpc_client, &miner_address, 1100).await;
    app.subscribe_covenant_addresses();
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Deploy
    app.handle_key(key_event(KeyCode::Char('d')));
    app.process_pending_ops().await;
    app.covenants = app.db.list_covenants();

    // Press 'x' — should open ConfirmDelete popup with WARNING
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Char('x')));
    if let InputMode::ConfirmDelete { ref lines, .. } = app.input_mode {
        assert!(lines.iter().any(|l| l.contains("WARNING")), "Should show WARNING for deployed covenant");
    } else {
        panic!("Expected ConfirmDelete popup");
    }

    // Press Esc to cancel — covenant should survive
    app.handle_input_key(key_event(KeyCode::Esc));
    assert!(app.input_mode.is_normal());
    assert_eq!(app.covenants.len(), 1, "Covenant should survive Esc cancel");

    // Press 'x' again, then 'y' to actually delete
    app.handle_key(key_event(KeyCode::Char('x')));
    app.handle_input_key(key_event(KeyCode::Char('y')));
    assert!(app.input_mode.is_normal());
    assert!(app.covenants.is_empty(), "Deployed covenant should be deleted after confirmation");

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test tx history tracking across multiple operations.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_tx_history_tracking() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, miner_address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create and select covenant
    app.handle_key(key_event(KeyCode::Char('c')));
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));

    // Fund deployer — mine 100 blocks to deployer (coinbases at DAA 1-100), then 1100 to miner
    // to advance DAA to ~1200 so ALL deployer coinbases satisfy simnet maturity of 1000.
    let deployer_addr_str = app.deployer_address(&app.covenants[0].1).unwrap();
    let deployer_addr: Address = deployer_addr_str.clone().try_into().unwrap();
    mine_blocks(&grpc_client, &deployer_addr, 100).await;
    mine_blocks(&grpc_client, &miner_address, 1100).await;
    app.subscribe_covenant_addresses();
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Deploy
    app.handle_key(key_event(KeyCode::Char('d')));
    app.process_pending_ops().await;

    assert_eq!(app.tx_history.len(), 1, "Should have deploy tx");
    assert_eq!(app.tx_history[0].action, "Deploy");

    // Create account and fund it — mine 100 to account then 1100 more to miner to mature.
    app.active_tab = Tab::Accounts;
    app.handle_key(key_event(KeyCode::Char('c')));
    let (pk, _) = app.accounts[0];
    let acct_addr = Address::new(Prefix::Simnet, Version::PubKey, &pk.as_bytes());
    mine_blocks(&grpc_client, &acct_addr, 100).await;
    mine_blocks(&grpc_client, &miner_address, 1100).await;
    app.subscribe_covenant_addresses();
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Do an entry
    app.active_tab = Tab::Actions;
    app.start_action_input(ActionType::Entry);
    app.handle_input_key(key_event(KeyCode::Char('5')));
    app.handle_input_key(key_event(KeyCode::Char('0')));
    app.handle_input_key(key_event(KeyCode::Char('0')));
    app.handle_input_key(key_event(KeyCode::Enter));
    app.handle_input_key(key_event(KeyCode::Enter));
    app.process_pending_ops().await;

    assert_eq!(app.tx_history.len(), 2, "Should have deploy + entry txs");
    assert_eq!(app.tx_history[1].action, "Entry (Deposit)");

    // Test tx_history_index navigation
    app.active_tab = Tab::TxHistory;
    assert_eq!(app.tx_history_index, 1); // Should point to last entry
    app.handle_key(key_event(KeyCode::Char('k'))); // Move up
    assert_eq!(app.tx_history_index, 0);
    app.handle_key(key_event(KeyCode::Char('j'))); // Move down
    assert_eq!(app.tx_history_index, 1);

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

// ── Role-based accounts tests ──

/// Test that deployer, prover, and accounts are separate roles.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_role_separation() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, _address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create covenant
    app.active_tab = Tab::Covenants;
    app.handle_key(key_event(KeyCode::Char('c')));
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));

    // Deployer address should exist (created covenant)
    let deployer_addr = app.deployer_address(&app.covenants[0].1);
    assert!(deployer_addr.is_some(), "Created covenant should have deployer address");

    // Prover key should be loaded
    assert!(app.prover_key.is_some(), "Prover key should be loaded after selecting covenant");
    let prover_addr = app.prover_address();
    assert!(prover_addr.is_some(), "Prover address should exist");

    // Accounts list should be empty (deployer/prover are NOT in accounts)
    assert!(app.accounts.is_empty(), "Accounts should be empty — deployer and prover are separate");

    // Create an action account
    app.active_tab = Tab::Accounts;
    app.handle_key(key_event(KeyCode::Char('c')));
    assert_eq!(app.accounts.len(), 1, "Should have exactly one account");

    // The account should be different from deployer and prover
    let (acct_pk, _) = app.accounts[0];
    let acct_addr = app.pubkey_to_address(&acct_pk).unwrap();
    assert_ne!(Some(acct_addr.clone()), deployer_addr, "Account address should differ from deployer");
    assert_ne!(Some(acct_addr), prover_addr, "Account address should differ from prover");

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test state tab 'r' key refetches chain data.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_state_tab_refetch() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, miner_address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create covenant
    app.handle_key(key_event(KeyCode::Char('c')));
    let deployer_addr_str = app.deployer_address(&app.covenants[0].1).unwrap();
    let deployer_addr: Address = deployer_addr_str.clone().try_into().unwrap();
    // Mine 100 blocks to deployer (coinbases at DAA 1-100), then 1100 to miner to advance DAA
    // to ~1200 so ALL deployer coinbases satisfy simnet maturity of 1000.
    mine_blocks(&grpc_client, &deployer_addr, 100).await;
    mine_blocks(&grpc_client, &miner_address, 1100).await;

    // Select, subscribe, deploy
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    app.active_tab = Tab::Covenants;
    app.handle_key(key_event(KeyCode::Char('d')));
    app.process_pending_ops().await;

    // Refresh and re-select to init prover
    app.covenants = app.db.list_covenants();
    app.prover = None;
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));
    assert!(app.prover.is_some(), "Prover should be initialized");

    // Process initial chain sync
    mine_blocks(&grpc_client, &miner_address, 5).await;
    app.process_pending_ops().await;

    let last_block_before = app.prover.as_ref().unwrap().last_processed_block;

    // Mine more blocks
    mine_blocks(&grpc_client, &miner_address, 5).await;

    // Switch to State tab and press 'r' to refetch
    app.active_tab = Tab::State;
    app.handle_key(key_event(KeyCode::Char('r')));
    app.process_pending_ops().await;

    // Prover should have advanced
    let last_block_after = app.prover.as_ref().unwrap().last_processed_block;
    assert_ne!(last_block_before, last_block_after, "Prover should have advanced after refetch");

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test that deploy fetches VCC + block header and records deploy_starting_block / deploy_initial_seq.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_deploy_sets_starting_block_and_seq() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, miner_address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create and select covenant
    app.active_tab = Tab::Covenants;
    app.handle_key(key_event(KeyCode::Char('c')));
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));

    // Fund deployer
    let deployer_addr_str = app.deployer_address(&app.covenants[0].1).unwrap();
    let deployer_addr: Address = deployer_addr_str.clone().try_into().unwrap();
    mine_blocks(&grpc_client, &deployer_addr, 100).await;
    mine_blocks(&grpc_client, &miner_address, 1100).await;

    app.subscribe_covenant_addresses();
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Deploy
    app.handle_key(key_event(KeyCode::Char('d')));
    app.process_pending_ops().await;

    // Reload from DB
    let covenant_id = app.covenants[0].0;
    let rec = app.db.get_covenant(covenant_id).unwrap().unwrap();
    assert!(rec.deployment_tx_id.is_some(), "Should have deployment tx ID");
    assert!(rec.deploy_starting_block.is_some(), "Should have deploy_starting_block");
    assert!(rec.deploy_initial_seq.is_some(), "Should have deploy_initial_seq");

    let starting_block = rec.deploy_starting_block.unwrap();
    let initial_seq = rec.deploy_initial_seq.unwrap();

    // Verify starting_block is NOT the pruning point (should be a recent confirmed block)
    assert_ne!(starting_block, app.pruning_point, "deploy_starting_block should not be pruning point");

    // Verify initial_seq matches the accepted_id_merkle_root of the starting block
    let block = app.node.get_block(starting_block, false).await.unwrap();
    assert_eq!(
        initial_seq, block.header.accepted_id_merkle_root,
        "deploy_initial_seq should equal block header accepted_id_merkle_root"
    );

    // Pressing 'd' again should be rejected (already deployed)
    let log_len = app.log_messages.len();
    app.covenants = app.db.list_covenants();
    app.handle_key(key_event(KeyCode::Char('d')));
    assert!(
        app.log_messages[log_len..].iter().any(|m| m.contains("already deployed")),
        "Second deploy should be rejected"
    );

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// Test that deploy_in_progress guard prevents duplicate deploys.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_deploy_duplicate_guard() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(simnet_args(), 10);
    let (mut app, grpc_client, miner_address, _keypair) = setup_test_app(&mut kaspad).await;

    // Create and select covenant
    app.active_tab = Tab::Covenants;
    app.handle_key(key_event(KeyCode::Char('c')));
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));

    // Fund deployer
    let deployer_addr_str = app.deployer_address(&app.covenants[0].1).unwrap();
    let deployer_addr: Address = deployer_addr_str.clone().try_into().unwrap();
    mine_blocks(&grpc_client, &deployer_addr, 100).await;
    mine_blocks(&grpc_client, &miner_address, 1100).await;

    app.subscribe_covenant_addresses();
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Press 'd' — starts deploy
    app.handle_key(key_event(KeyCode::Char('d')));
    assert!(app.deploy_in_progress, "deploy_in_progress should be true after pressing 'd'");

    // Press 'd' again — should be rejected
    let log_len = app.log_messages.len();
    app.handle_key(key_event(KeyCode::Char('d')));
    assert!(
        app.log_messages[log_len..].iter().any(|m| m.contains("already in progress")),
        "Second press should be rejected while deploy in progress"
    );

    // Let the deploy complete
    app.process_pending_ops().await;
    assert!(!app.deploy_in_progress, "deploy_in_progress should be false after completion");

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}

/// End-to-end test: deploy → mine → chain sync → prove (real ZK) → submit → mine → prove → submit.
///
/// Uses zero coinbase maturity so UTXOs are immediately spendable.
/// The first run computes actual proofs (slow). Once passing, captured data can be hardcoded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore] // Remove once proof constants are hardcoded — first run is slow (computes real ZK proofs)
async fn test_deploy_prove_submit_cycle() {
    kaspa_core::log::try_init_logger("INFO");

    let mut kaspad = Daemon::new_random_with_args(zero_maturity_args(), 10);
    let grpc_client = kaspad.start().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let url = format!("ws://localhost:{}", kaspad.rpc_borsh_port);
    let network_id = NetworkId::new(NetworkType::Simnet);
    let node = KaspaNode::try_new(&url, network_id).unwrap();
    node.connect().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let dag_info = node.get_block_dag_info().await.unwrap();
    let tmpdir = tempfile::tempdir().unwrap();
    let db = Arc::new(RollupDb::open(tmpdir.path()).unwrap());
    let mut app = App::new(db, node, Prefix::Simnet);
    app.pruning_point = dag_info.pruning_point_hash;
    app.connected = true;

    let keypair = secp256k1::Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
    let miner_address = Address::new(Prefix::Simnet, Version::PubKey, &keypair.x_only_public_key().0.serialize());

    // ── Create and select covenant ──
    app.active_tab = Tab::Covenants;
    app.handle_key(key_event(KeyCode::Char('c')));
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));

    // ── Fund deployer (zero maturity = immediately spendable) ──
    let deployer_addr_str = app.deployer_address(&app.covenants[0].1).unwrap();
    let deployer_addr: Address = deployer_addr_str.clone().try_into().unwrap();
    // With zero coinbase maturity, deployer UTXOs are immediately spendable.
    // Still need ~1100 total blocks for VCC min_confirmations=1000.
    mine_blocks(&grpc_client, &deployer_addr, 10).await;
    mine_blocks(&grpc_client, &miner_address, 1100).await;

    app.subscribe_covenant_addresses();
    app.process_pending_ops().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let balance = app.utxo_tracker.balance(&deployer_addr_str);
    assert!(balance > 0, "Deployer should have funds, got: {balance}");

    // ── Deploy ──
    app.handle_key(key_event(KeyCode::Char('d')));
    app.process_pending_ops().await;

    app.covenants = app.db.list_covenants();
    let covenant_id = app.covenants[0].0;
    let rec = app.db.get_covenant(covenant_id).unwrap().unwrap();
    assert!(rec.deployment_tx_id.is_some(), "Covenant should be deployed");
    assert!(rec.deploy_starting_block.is_some(), "Should have deploy_starting_block");
    assert!(rec.deploy_initial_seq.is_some(), "Should have deploy_initial_seq");

    let deploy_starting_block = rec.deploy_starting_block.unwrap();
    let deploy_initial_seq = rec.deploy_initial_seq.unwrap();

    // Verify initial_seq matches the block header
    let block = app.node.get_block(deploy_starting_block, false).await.unwrap();
    assert_eq!(deploy_initial_seq, block.header.accepted_id_merkle_root, "deploy_initial_seq mismatch");

    // ── Re-select to init prover ──
    app.prover = None;
    app.covenant_list_index = 0;
    app.handle_key(key_event(KeyCode::Enter));
    assert!(app.prover.is_some(), "Prover should be initialized");

    // ── Mine blocks so chain sync has data ──
    mine_blocks(&grpc_client, &miner_address, 10).await;

    // ── Chain sync (fetches VCC v2, processes blocks) ──
    app.active_tab = Tab::Proving;
    app.handle_key(key_event(KeyCode::Char('p'))); // start_chain_processing
    app.process_pending_ops().await;

    let accumulated = app.prover.as_ref().unwrap().accumulated_blocks();
    assert!(accumulated > 0, "Should have accumulated blocks for proving, got: {accumulated}");


    // ── Proof 1: generate actual ZK proof (IPC backend → external r0vm) ──
    app.prover_backend = zk_covenant_rollup_host::prove::ProverBackend::Ipc;
    app.handle_key(key_event(KeyCode::Char('r'))); // start_proving
    assert!(app.proof_in_progress, "Proof should be in progress");

    // Wait for proof completion (real computation, may take minutes)
    app.wait_for_proof(Duration::from_secs(600)).await;

    assert!(!app.completed_proofs.is_empty(), "Should have a completed proof");
    assert!(app.last_proof_result.as_ref().unwrap().contains("completed"), "Proof should have completed successfully");

    // ── Submit proof 1 ──
    app.handle_key(key_event(KeyCode::Char('s'))); // submit_proof
    app.process_pending_ops().await;

    let prove_txs: Vec<_> = app.tx_history.iter().filter(|t| t.action == "Prove").collect();
    assert_eq!(prove_txs.len(), 1, "Should have one prove tx after first submission");

    // Verify the covenant UTXO was updated
    app.covenants = app.db.list_covenants();
    let rec_after_proof1 = app.db.get_covenant(covenant_id).unwrap().unwrap();
    assert_ne!(
        rec_after_proof1.covenant_utxo, rec.covenant_utxo,
        "Covenant UTXO should have changed after proof submission"
    );

    // ── Mine more blocks for second proving window ──
    mine_blocks(&grpc_client, &miner_address, 10).await;

    // ── Chain sync again ──
    app.handle_key(key_event(KeyCode::Char('p')));
    app.process_pending_ops().await;

    let accumulated2 = app.prover.as_ref().unwrap().accumulated_blocks();
    assert!(accumulated2 > 0, "Should have new accumulated blocks for second proof, got: {accumulated2}");

    // ── Proof 2 ──
    app.handle_key(key_event(KeyCode::Char('r')));
    assert!(app.proof_in_progress, "Second proof should be in progress");
    app.wait_for_proof(Duration::from_secs(600)).await;

    assert!(!app.completed_proofs.is_empty(), "Should have second completed proof");

    // ── Submit proof 2 ──
    app.handle_key(key_event(KeyCode::Char('s')));
    app.process_pending_ops().await;

    let prove_txs: Vec<_> = app.tx_history.iter().filter(|t| t.action == "Prove").collect();
    assert_eq!(prove_txs.len(), 2, "Should have two prove txs after second submission");

    // Verify the covenant UTXO advanced again
    app.covenants = app.db.list_covenants();
    let rec_after_proof2 = app.db.get_covenant(covenant_id).unwrap().unwrap();
    assert_ne!(
        rec_after_proof2.covenant_utxo, rec_after_proof1.covenant_utxo,
        "Covenant UTXO should have changed after second proof submission"
    );

    // Verify no failures in tx history
    for tx in &app.tx_history {
        assert!(
            !matches!(tx.status, zk_covenant_rollup_tui::app::TxStatus::Failed(_)),
            "Tx {} ({}) should not have failed: {:?}",
            tx.tx_id,
            tx.action,
            tx.status
        );
    }

    app.node.stop().await.unwrap();
    grpc_client.disconnect().await.unwrap();
    drop(grpc_client);
    kaspad.shutdown();
}
