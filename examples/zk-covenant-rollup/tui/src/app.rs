use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::sign::sign;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{
    SignableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_hashes::Hash;
use kaspa_rpc_core::RpcTransaction;
use kaspa_txscript::zk_precompiles::tags::ZkTag;
use kaspa_txscript::{pay_to_address_script, pay_to_script_hash_script};
use kaspa_wrpc_client::prelude::Notification;
use tokio::sync::mpsc;
use zk_covenant_rollup_core::state::empty_tree_root;
use zk_covenant_rollup_host::mock_chain::{calc_accepted_id_merkle_root, from_bytes};
use zk_covenant_rollup_host::redeem::build_redeem_script;
use zk_covenant_rollup_methods::ZK_COVENANT_ROLLUP_GUEST_ID;

use zk_covenant_rollup_host::prove::{self as host_prove, ProofKind, ProveInput, ProverBackend};

use crate::balance::UtxoTracker;
use crate::db::{CovenantId, CovenantRecord, ProvingState, Pubkey, RollupDb};
use crate::node::{KaspaNode, NodeEvent};
use crate::prover::RollupProver;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Covenants,
    Accounts,
    Actions,
    State,
    Proving,
    TxHistory,
    Log,
}

impl Tab {
    pub fn title(&self) -> &'static str {
        match self {
            Tab::Covenants => "1:Covenants",
            Tab::Accounts => "2:Accounts",
            Tab::Actions => "3:Actions",
            Tab::State => "4:State",
            Tab::Proving => "5:Proving",
            Tab::TxHistory => "6:TxHistory",
            Tab::Log => "7:Log",
        }
    }

    pub fn all() -> &'static [Tab] {
        &[Tab::Covenants, Tab::Accounts, Tab::Actions, Tab::State, Tab::Proving, Tab::TxHistory, Tab::Log]
    }

    pub fn index(&self) -> usize {
        Tab::all().iter().position(|t| t == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    Entry,
    Transfer,
    Exit,
}

impl ActionType {
    pub fn label(&self) -> &'static str {
        match self {
            ActionType::Entry => "Entry (Deposit)",
            ActionType::Transfer => "Transfer",
            ActionType::Exit => "Exit (Withdrawal)",
        }
    }

    pub fn unit(&self) -> &'static str {
        match self {
            ActionType::Entry => "sompi",
            ActionType::Transfer | ActionType::Exit => "L2 units",
        }
    }
}

pub enum TextInputTarget {
    ImportCovenantId,
    ImportDeployTxId { covenant_id: Hash },
}

pub enum InputMode {
    Normal,
    PromptAmount { action: ActionType, buffer: String, context: String },
    PromptText { target: TextInputTarget, buffer: String, context: String },
    Confirm { action: ActionType, amount: u64, summary: Vec<String> },
    Processing { action: ActionType },
}

impl InputMode {
    pub fn is_normal(&self) -> bool {
        matches!(self, InputMode::Normal)
    }
}

#[derive(Clone)]
pub struct TxRecord {
    pub tx_id: Hash,
    pub action: String,
    pub amount: u64,
    pub timestamp: u64,
    pub status: TxStatus,
}

#[derive(Clone, PartialEq, Eq)]
pub enum TxStatus {
    Submitted,
    Confirmed,
    Failed(String),
}

/// Results delivered from background tasks back to the main event loop.
enum BgResult {
    UtxosFetched { entries: Vec<kaspa_rpc_core::RpcUtxosByAddressesEntry>, address_count: usize },
    UtxosFetchFailed(String),
    UtxoSubscribeFailed(String),
    ChainFetched(kaspa_rpc_core::GetVirtualChainFromBlockV2Response),
    ChainFetchFailed(String),
    TxSubmitted { tx_id: Hash },
    TxSubmitFailed { tx_id: Hash, error: String },
    ActionBuilt { action: ActionType, amount: u64, tx: Transaction },
    ActionBuildFailed { action: ActionType, error: String },
    ProofCompleted { gen: u64, elapsed_ms: u128, segments: usize, total_cycles: u64 },
    ProofFailed { gen: u64, error: String },
}

pub struct App {
    pub db: Arc<RollupDb>,
    pub node: KaspaNode,
    pub network_prefix: Prefix,
    pub active_tab: Tab,
    pub daa_score: u64,
    pub connected: bool,
    pub should_quit: bool,
    pub log_messages: Vec<String>,

    // Covenant tab state
    pub covenants: Vec<(CovenantId, CovenantRecord)>,
    pub covenant_list_index: usize,
    pub selected_covenant: Option<usize>,

    // Account tab state (loaded for selected covenant)
    pub accounts: Vec<(Pubkey, [u8; 32])>, // (pubkey, privkey)
    pub account_list_index: usize,

    // Prover key (separate from deployer — for proving role)
    pub prover_key: Option<(Pubkey, [u8; 32])>, // (pubkey, privkey)

    // Actions tab state
    pub action_menu_index: usize,
    pub input_mode: InputMode,

    // Transaction history
    pub tx_history: Vec<TxRecord>,
    pub tx_history_index: usize,

    // Balance tracking
    pub utxo_tracker: UtxoTracker,

    // Proving state
    pub prover: Option<RollupProver>,
    pub proving_status: String,
    pub pruning_point: Hash,
    pub prover_backend: ProverBackend,
    pub proof_kind: ProofKind,
    pub proof_in_progress: bool,
    pub last_proof_result: Option<String>,
    /// Monotonic counter — incremented on each prove start and on cancel.
    /// Results from older generations are discarded.
    proof_generation: u64,

    /// Pending operations queued by sync key handlers, dispatched to background tasks.
    pending_ops: Vec<PendingOp>,

    /// Channel for receiving results from background tasks.
    bg_tx: mpsc::UnboundedSender<BgResult>,
    bg_rx: mpsc::UnboundedReceiver<BgResult>,

    /// True while a FetchAndProcessChain task is in-flight (prevents double-firing).
    chain_sync_active: bool,
}

/// Deferred async operations triggered from sync key handlers.
enum PendingOp {
    SubscribeAndFetchUtxos(Vec<Address>),
    SubmitTransaction(Transaction),
    FetchAndProcessChain,
    BuildAndSubmitAction { action: ActionType, amount: u64 },
    GenerateProof { gen: u64, input: ProveInput, backend: ProverBackend, kind: ProofKind },
}

impl App {
    pub fn new(db: Arc<RollupDb>, node: KaspaNode, network_prefix: Prefix) -> Self {
        let covenants = db.list_covenants();
        let (bg_tx, bg_rx) = mpsc::unbounded_channel();
        Self {
            db,
            node,
            network_prefix,
            active_tab: Tab::Covenants,
            daa_score: 0,
            connected: false,
            should_quit: false,
            log_messages: Vec::new(),
            covenants,
            covenant_list_index: 0,
            selected_covenant: None,
            accounts: Vec::new(),
            account_list_index: 0,
            prover_key: None,
            action_menu_index: 0,
            input_mode: InputMode::Normal,
            tx_history: Vec::new(),
            tx_history_index: 0,
            utxo_tracker: UtxoTracker::new(),
            prover: None,
            proving_status: "No covenant selected".into(),
            pruning_point: Hash::default(),
            prover_backend: ProverBackend::Cpu,
            proof_kind: ProofKind::Succinct,
            proof_in_progress: false,
            last_proof_result: None,
            proof_generation: 0,
            pending_ops: Vec::new(),
            bg_tx,
            bg_rx,
            chain_sync_active: false,
        }
    }

    pub async fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> anyhow::Result<()> {
        let event_rx = self.node.event_receiver();

        loop {
            terminal.draw(|frame| crate::ui::draw(frame, self))?;

            // Poll for crossterm events with 100ms timeout.
            // Background tasks run on other tokio threads during this wait.
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                }
            }

            // Drain node events (non-blocking)
            while let Ok(ev) = event_rx.try_recv() {
                self.handle_node_event(ev);
            }

            // Drain background task results (non-blocking)
            while let Ok(result) = self.bg_rx.try_recv() {
                self.handle_bg_result(result);
            }

            // Continuous chain sync: re-schedule when prover exists and no fetch in-flight
            if self.prover.is_some() && !self.chain_sync_active {
                self.pending_ops.push(PendingOp::FetchAndProcessChain);
            }

            // Dispatch queued operations to background tasks
            self.dispatch_pending_ops();

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// For tests: dispatch all pending ops to background, then poll until they complete.
    pub async fn process_pending_ops(&mut self) {
        self.dispatch_pending_ops();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            // Yield so spawned tasks can make progress
            tokio::time::sleep(Duration::from_millis(10)).await;
            while let Ok(result) = self.bg_rx.try_recv() {
                self.handle_bg_result(result);
            }
            // Dispatch any newly queued ops (e.g., SubmitTransaction after ActionBuilt)
            if !self.pending_ops.is_empty() {
                self.dispatch_pending_ops();
                continue;
            }
            if !self.chain_sync_active {
                break;
            }
            if tokio::time::Instant::now() > deadline {
                break;
            }
        }
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        // If input mode is active, route to input handler first
        if !self.input_mode.is_normal() {
            self.handle_input_key(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.should_quit = true,

            // Tab switching
            KeyCode::Char('1') => self.active_tab = Tab::Covenants,
            KeyCode::Char('2') => self.active_tab = Tab::Accounts,
            KeyCode::Char('3') => self.active_tab = Tab::Actions,
            KeyCode::Char('4') => self.active_tab = Tab::State,
            KeyCode::Char('5') => self.active_tab = Tab::Proving,
            KeyCode::Char('6') => self.active_tab = Tab::TxHistory,
            KeyCode::Char('7') => self.active_tab = Tab::Log,

            // Tab-specific keys
            _ => self.handle_tab_key(key),
        }
    }

    fn handle_tab_key(&mut self, key: crossterm::event::KeyEvent) {
        match self.active_tab {
            Tab::Covenants => self.handle_covenant_key(key),
            Tab::Accounts => self.handle_account_key(key),
            Tab::Actions => self.handle_action_key(key),
            Tab::State => self.handle_state_key(key),
            Tab::Proving => self.handle_proving_key(key),
            Tab::TxHistory => self.handle_tx_history_key(key),
            _ => {}
        }
    }

    // ── Covenant tab ──

    fn handle_covenant_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char('c') => self.create_covenant(),
            KeyCode::Char('d') => self.deploy_covenant(),
            KeyCode::Char('i') => self.start_import_covenant(),
            KeyCode::Char('x') => self.delete_covenant(),
            KeyCode::Char('y') => self.copy_covenant_info(),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.covenant_list_index > 0 {
                    self.covenant_list_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.covenants.is_empty() && self.covenant_list_index < self.covenants.len() - 1 {
                    self.covenant_list_index += 1;
                }
            }
            KeyCode::Enter => {
                if !self.covenants.is_empty() {
                    self.selected_covenant = Some(self.covenant_list_index);
                    let id = self.covenants[self.covenant_list_index].0;
                    let is_deployed = self.covenants[self.covenant_list_index].1.deployment_tx_id.is_some();
                    self.refresh_accounts();
                    self.load_prover_key(id);
                    self.subscribe_covenant_addresses();
                    self.log(format!("Selected covenant: {id}"));

                    // Auto-init prover if covenant is deployed
                    if is_deployed && self.prover.is_none() {
                        let initial_state_root = zk_covenant_rollup_core::state::empty_tree_root();
                        let initial_seq =
                            zk_covenant_rollup_host::mock_chain::calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
                        self.prover = Some(RollupProver::new(id, initial_state_root, initial_seq, self.pruning_point));
                        self.log("Auto-initialized prover for deployed covenant".into());
                        self.pending_ops.push(PendingOp::FetchAndProcessChain);
                    }
                }
            }
            _ => {}
        }
    }

    fn create_covenant(&mut self) {
        let secp = secp256k1::Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let (xonly_pk, _) = public_key.x_only_public_key();

        // Random covenant ID
        let mut id_bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut id_bytes);
        let covenant_id = Hash::from_bytes(id_bytes);

        let address = Address::new(self.network_prefix, Version::PubKey, &xonly_pk.serialize());

        let record = CovenantRecord {
            deployer_privkey: secret_key.secret_bytes().to_vec(),
            deployment_tx_id: None,
            covenant_utxo: None,
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        };

        if let Err(e) = self.db.put_covenant(covenant_id, &record) {
            self.log(format!("Failed to save covenant: {e}"));
            return;
        }

        // Store same key as prover key (creator gets both roles)
        if let Err(e) = self.db.put_prover_key(covenant_id, &secret_key.secret_bytes()) {
            self.log(format!("Failed to save prover key: {e}"));
            return;
        }

        self.log(format!("Created covenant {covenant_id} | deployer: {address}"));

        // Refresh list
        self.covenants = self.db.list_covenants();
        self.covenant_list_index = self.covenants.len().saturating_sub(1);
    }

    fn deploy_covenant(&mut self) {
        // Must have a selected covenant
        let cov_idx = match self.selected_covenant {
            Some(i) => i,
            None => {
                self.log("Select a covenant first (tab 1, Enter)".into());
                return;
            }
        };

        let (covenant_id, ref record) = self.covenants[cov_idx];

        // Must not already be deployed
        if record.deployment_tx_id.is_some() {
            self.log("Covenant is already deployed".into());
            return;
        }

        // Imported covenants have no deployer key
        if record.deployer_privkey.len() != 32 {
            self.log("Cannot deploy — no deployer key (imported covenant)".into());
            return;
        }

        // Get deployer keypair
        let deployer_sk = match secp256k1::SecretKey::from_slice(&record.deployer_privkey) {
            Ok(sk) => sk,
            Err(e) => {
                self.log(format!("Invalid deployer key: {e}"));
                return;
            }
        };
        let deployer_pk = deployer_sk.public_key(secp256k1::SECP256K1);
        let (xonly_pk, _) = deployer_pk.x_only_public_key();
        let deployer_addr = Address::new(self.network_prefix, Version::PubKey, &xonly_pk.serialize());
        let deployer_addr_str = deployer_addr.to_string();
        let deployer_spk = pay_to_address_script(&deployer_addr);

        // Select UTXOs from deployer
        let covenant_value: u64 = 100_000; // 0.001 KAS
        let fee: u64 = 10_000;
        let needed = covenant_value + fee;

        let (selected_utxos, total_input) = match self.utxo_tracker.select_utxos(&deployer_addr_str, needed) {
            Some(r) => r,
            None => {
                self.log(format!("Insufficient funds at deployer address {deployer_addr_str} (need {needed} sompi)"));
                return;
            }
        };

        // Build initial redeem script
        let prev_state_hash = empty_tree_root();
        let prev_seq_commitment = from_bytes(calc_accepted_id_merkle_root(Hash::default(), std::iter::empty()).as_bytes());
        let program_id: [u8; 32] = bytemuck::cast(ZK_COVENANT_ROLLUP_GUEST_ID);
        let zk_tag = ZkTag::R0Succinct;

        // Convergence loop for script length
        let mut computed_len: i64 = 75;
        loop {
            let script = build_redeem_script(prev_state_hash, prev_seq_commitment, computed_len, &program_id, &zk_tag);
            let new_len = script.len() as i64;
            if new_len == computed_len {
                break;
            }
            computed_len = new_len;
        }

        let redeem_script = build_redeem_script(prev_state_hash, prev_seq_commitment, computed_len, &program_id, &zk_tag);
        let covenant_spk = pay_to_script_hash_script(&redeem_script);

        // Build inputs
        let inputs: Vec<TransactionInput> =
            selected_utxos.iter().map(|u| TransactionInput::new(TransactionOutpoint::new(u.tx_id, u.index), vec![], 0, 0)).collect();

        let utxo_entries: Vec<UtxoEntry> =
            selected_utxos.iter().map(|u| UtxoEntry::new(u.amount, deployer_spk.clone(), 0, false, None)).collect();

        // Build outputs
        let mut outputs = vec![TransactionOutput::new(covenant_value, covenant_spk)];
        let change = total_input - needed;
        if change > 0 {
            outputs.push(TransactionOutput::new(change, deployer_spk.clone()));
        }

        let tx = Transaction::new(0, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let signable = SignableTransaction::with_entries(tx, utxo_entries);

        // Sign
        let keypair = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &deployer_sk);
        let signed = sign(signable, keypair);
        let tx_id = signed.id();

        // Update DB
        let mut updated_record = record.clone();
        updated_record.deployment_tx_id = Some(tx_id);
        updated_record.covenant_utxo = Some((tx_id, 0));
        if let Err(e) = self.db.put_covenant(covenant_id, &updated_record) {
            self.log(format!("Failed to update covenant in DB: {e}"));
            return;
        }

        self.log(format!("Deploying covenant {covenant_id} — tx: {tx_id}"));
        self.record_tx(tx_id, "Deploy", covenant_value);
        self.pending_ops.push(PendingOp::SubmitTransaction(signed.tx));

        // Refresh list
        self.covenants = self.db.list_covenants();
    }

    fn delete_covenant(&mut self) {
        if self.covenants.is_empty() {
            self.log("No covenants to delete".into());
            return;
        }

        let (id, ref record) = self.covenants[self.covenant_list_index];

        if record.deployment_tx_id.is_some() {
            self.log("Cannot delete a deployed covenant".into());
            return;
        }

        if let Err(e) = self.db.delete_covenant(id) {
            self.log(format!("Failed to delete covenant: {e}"));
            return;
        }

        let deleted_idx = self.covenant_list_index;
        self.log(format!("Deleted covenant {id}"));
        self.covenants = self.db.list_covenants();

        // Adjust cursor
        if self.covenants.is_empty() {
            self.covenant_list_index = 0;
            self.selected_covenant = None;
        } else {
            if self.covenant_list_index >= self.covenants.len() {
                self.covenant_list_index = self.covenants.len() - 1;
            }
            // Adjust selected_covenant index
            if let Some(sel) = self.selected_covenant {
                if sel == deleted_idx {
                    self.selected_covenant = None;
                } else if sel > deleted_idx {
                    self.selected_covenant = Some(sel - 1);
                }
            }
        }
    }

    // ── Account tab ──

    fn handle_account_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char('c') => self.create_account(),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.account_list_index > 0 {
                    self.account_list_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.accounts.is_empty() && self.account_list_index < self.accounts.len() - 1 {
                    self.account_list_index += 1;
                }
            }
            _ => {}
        }
    }

    fn create_account(&mut self) {
        let cov_idx = match self.selected_covenant {
            Some(i) => i,
            None => {
                self.log("Select a covenant first (tab 1, Enter)".into());
                return;
            }
        };
        let covenant_id = self.covenants[cov_idx].0;

        let secp = secp256k1::Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let (xonly_pk, _) = public_key.x_only_public_key();
        let pk_bytes = xonly_pk.serialize();
        let index = pk_bytes[0];

        // Check for first-byte collision
        for (existing_pk, _) in &self.accounts {
            if existing_pk.as_bytes()[0] == index {
                self.log(format!("Index 0x{index:02x} already taken, try again"));
                return;
            }
        }

        let pubkey = Hash::from_bytes(pk_bytes);
        let privkey = secret_key.secret_bytes();
        let address = Address::new(self.network_prefix, Version::PubKey, &pk_bytes);

        if let Err(e) = self.db.put_account_key(covenant_id, pubkey, &privkey) {
            self.log(format!("Failed to save account: {e}"));
            return;
        }

        self.log(format!("Created account idx=0x{index:02x} addr={address}"));
        self.refresh_accounts();
        self.subscribe_covenant_addresses();
        self.account_list_index = self.accounts.len().saturating_sub(1);
    }

    fn refresh_accounts(&mut self) {
        if let Some(i) = self.selected_covenant {
            let covenant_id = self.covenants[i].0;
            self.accounts = self.db.list_accounts(covenant_id);
            self.account_list_index = 0;
        }
    }

    /// Collect all tracked addresses for the selected covenant and schedule UTXO subscription.
    pub fn subscribe_covenant_addresses(&mut self) {
        let cov_idx = match self.selected_covenant {
            Some(i) => i,
            None => return,
        };

        let mut addresses = Vec::new();

        // Deployer address
        if let Some(addr) = self.deployer_address_obj(&self.covenants[cov_idx].1) {
            addresses.push(addr);
        }

        // Prover address
        if let Some((pk, _)) = &self.prover_key {
            addresses.push(Address::new(self.network_prefix, Version::PubKey, &pk.as_bytes()));
        }

        // Account addresses
        for (pubkey, _) in &self.accounts {
            let addr = Address::new(self.network_prefix, Version::PubKey, &pubkey.as_bytes());
            addresses.push(addr);
        }

        if !addresses.is_empty() {
            self.pending_ops.push(PendingOp::SubscribeAndFetchUtxos(addresses));
        }
    }

    // ── Actions tab ──

    fn handle_action_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char('e') => self.start_action_input(ActionType::Entry),
            KeyCode::Char('t') => self.start_action_input(ActionType::Transfer),
            KeyCode::Char('x') => self.start_action_input(ActionType::Exit),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.action_menu_index > 0 {
                    self.action_menu_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.action_menu_index < 2 {
                    self.action_menu_index += 1;
                }
            }
            _ => {}
        }
    }

    pub fn start_action_input(&mut self, action: ActionType) {
        // Validate prerequisites
        let cov_idx = match self.selected_covenant {
            Some(i) => i,
            None => {
                self.log("Select a covenant first".into());
                return;
            }
        };
        if self.accounts.is_empty() {
            self.log("Create an account first".into());
            return;
        }
        if action == ActionType::Transfer && self.accounts.len() < 2 {
            self.log("Need at least 2 accounts for a transfer".into());
            return;
        }

        let (pk, _) = self.accounts[self.account_list_index];
        let addr_str = self.pubkey_to_address(&pk).unwrap_or_default();
        let l1_balance = self.utxo_tracker.balance(&addr_str);

        // Check gas UTXOs
        let utxos = self.utxo_tracker.available_utxos(&addr_str);
        if utxos.is_empty() && action == ActionType::Entry {
            self.log(format!("No UTXOs available for {addr_str} — fund this address first"));
            return;
        }
        if action == ActionType::Transfer || action == ActionType::Exit {
            let source_addr = self.pubkey_to_address(&self.accounts[0].0).unwrap_or_default();
            let source_utxos = self.utxo_tracker.available_utxos(&source_addr);
            if source_utxos.is_empty() {
                self.log(format!("No UTXOs for source account {source_addr}"));
                return;
            }
        }

        // Build L2 balance context
        let l2_balance = self
            .prover
            .as_ref()
            .map(|p| {
                let pk_words = zk_covenant_rollup_host::mock_chain::from_bytes(pk.as_bytes());
                p.smt.get(&pk_words).unwrap_or(0)
            })
            .unwrap_or(0);

        let _cov_id = self.covenants[cov_idx].0;

        let context = match action {
            ActionType::Entry => format!(
                "Account idx=0x{:02x} | L1: {} sompi | L2: {}\nEnter deposit amount in {}:",
                pk.as_bytes()[0],
                l1_balance,
                l2_balance,
                action.unit()
            ),
            ActionType::Transfer => {
                let (src_pk, _) = self.accounts[0];
                let (dst_pk, _) = self.accounts[1];
                let src_l2 = self
                    .prover
                    .as_ref()
                    .map(|p| {
                        let w = zk_covenant_rollup_host::mock_chain::from_bytes(src_pk.as_bytes());
                        p.smt.get(&w).unwrap_or(0)
                    })
                    .unwrap_or(0);
                format!(
                    "From idx=0x{:02x} (L2: {}) → To idx=0x{:02x}\nEnter transfer amount in {}:",
                    src_pk.as_bytes()[0],
                    src_l2,
                    dst_pk.as_bytes()[0],
                    action.unit()
                )
            }
            ActionType::Exit => {
                format!("Account idx=0x{:02x} | L2: {}\nEnter withdrawal amount in {}:", pk.as_bytes()[0], l2_balance, action.unit())
            }
        };

        self.input_mode = InputMode::PromptAmount { action, buffer: String::new(), context };
    }

    pub fn handle_input_key(&mut self, key: crossterm::event::KeyEvent) {
        // Handle PromptText separately to avoid borrow checker issues
        if matches!(self.input_mode, InputMode::PromptText { .. }) {
            self.handle_prompt_text_key(key);
            return;
        }

        match &mut self.input_mode {
            InputMode::Normal => {}
            InputMode::PromptAmount { action, buffer, .. } => {
                let action = *action;
                match key.code {
                    KeyCode::Char(c) if c.is_ascii_digit() => buffer.push(c),
                    KeyCode::Backspace => {
                        buffer.pop();
                    }
                    KeyCode::Enter => {
                        let amount: u64 = match buffer.parse() {
                            Ok(v) if v > 0 => v,
                            _ => {
                                self.log("Invalid amount — enter a positive number".into());
                                return;
                            }
                        };
                        let summary = self.build_action_summary(action, amount);
                        self.input_mode = InputMode::Confirm { action, amount, summary };
                    }
                    KeyCode::Esc => {
                        self.input_mode = InputMode::Normal;
                        self.log("Action cancelled".into());
                    }
                    _ => {}
                }
            }
            InputMode::Confirm { action, amount, .. } => {
                let action = *action;
                let amount = *amount;
                match key.code {
                    KeyCode::Enter => {
                        self.input_mode = InputMode::Processing { action };
                        self.pending_ops.push(PendingOp::BuildAndSubmitAction { action, amount });
                    }
                    KeyCode::Esc => {
                        self.input_mode = InputMode::Normal;
                        self.log("Action cancelled".into());
                    }
                    _ => {}
                }
            }
            InputMode::Processing { .. } => {
                // Ignore keys while processing
            }
            InputMode::PromptText { .. } => unreachable!("handled above"),
        }
    }

    fn build_action_summary(&self, action: ActionType, amount: u64) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!("Action: {}", action.label()));
        lines.push(format!("Amount: {} {}", amount, action.unit()));

        match action {
            ActionType::Entry => {
                if let Some(idx) = self.account_list_index.checked_add(0) {
                    if let Some((pk, _)) = self.accounts.get(idx) {
                        lines.push(format!("Destination: idx=0x{:02x}", pk.as_bytes()[0]));
                    }
                }
            }
            ActionType::Transfer => {
                if self.accounts.len() >= 2 {
                    lines.push(format!("From: idx=0x{:02x}", self.accounts[0].0.as_bytes()[0]));
                    lines.push(format!("To:   idx=0x{:02x}", self.accounts[1].0.as_bytes()[0]));
                }
            }
            ActionType::Exit => {
                if let Some((pk, _)) = self.accounts.get(self.account_list_index) {
                    lines.push(format!("Source: idx=0x{:02x}", pk.as_bytes()[0]));
                }
            }
        }

        lines.push(String::new());
        lines.push("Enter: submit | Esc: cancel".into());
        lines
    }

    // ── State tab ──

    fn handle_state_key(&mut self, key: crossterm::event::KeyEvent) {
        if let KeyCode::Char('r') = key.code {
            if self.prover.is_some() {
                self.pending_ops.push(PendingOp::FetchAndProcessChain);
                self.log("Refetching chain data...".into());
            } else {
                self.log("No prover initialized — select a deployed covenant first".into());
            }
        }
    }

    // ── Text input (import covenant) ──

    fn handle_prompt_text_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char(c) if c.is_ascii_hexdigit() => {
                if let InputMode::PromptText { buffer, .. } = &mut self.input_mode {
                    if buffer.len() < 64 {
                        buffer.push(c);
                    }
                }
            }
            KeyCode::Backspace => {
                if let InputMode::PromptText { buffer, .. } = &mut self.input_mode {
                    buffer.pop();
                }
            }
            KeyCode::Enter => {
                // Extract buffer contents before mutating self
                let (buf_len, buf_clone, target_clone) = match &self.input_mode {
                    InputMode::PromptText { buffer, target, .. } => {
                        let t = match target {
                            TextInputTarget::ImportCovenantId => None,
                            TextInputTarget::ImportDeployTxId { covenant_id } => Some(*covenant_id),
                        };
                        (buffer.len(), buffer.clone(), t)
                    }
                    _ => return,
                };

                if buf_len != 64 {
                    self.log(format!("Need exactly 64 hex chars, got {buf_len}"));
                    return;
                }
                let mut bytes = [0u8; 32];
                if faster_hex::hex_decode(buf_clone.as_bytes(), &mut bytes).is_err() {
                    self.log("Invalid hex string".into());
                    return;
                }
                let hash = Hash::from_bytes(bytes);
                match target_clone {
                    None => {
                        // Was ImportCovenantId -> advance to ImportDeployTxId
                        self.input_mode = InputMode::PromptText {
                            target: TextInputTarget::ImportDeployTxId { covenant_id: hash },
                            buffer: String::new(),
                            context: "Enter deploy tx ID (64 hex chars):".into(),
                        };
                    }
                    Some(covenant_id) => {
                        // Was ImportDeployTxId -> finish import
                        self.input_mode = InputMode::Normal;
                        self.finish_import_covenant(covenant_id, hash);
                    }
                }
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.log("Import cancelled".into());
            }
            _ => {}
        }
    }

    // ── Import covenant ──

    fn start_import_covenant(&mut self) {
        self.input_mode = InputMode::PromptText {
            target: TextInputTarget::ImportCovenantId,
            buffer: String::new(),
            context: "Enter covenant ID (64 hex chars):".into(),
        };
    }

    fn finish_import_covenant(&mut self, covenant_id: Hash, deploy_tx_id: Hash) {
        // Generate new prover keypair
        let secp = secp256k1::Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let (xonly_pk, _) = public_key.x_only_public_key();
        let prover_addr = Address::new(self.network_prefix, Version::PubKey, &xonly_pk.serialize());

        let record = CovenantRecord {
            deployer_privkey: vec![], // imported — no deployer key
            deployment_tx_id: Some(deploy_tx_id),
            covenant_utxo: Some((deploy_tx_id, 0)),
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        };

        if let Err(e) = self.db.put_covenant(covenant_id, &record) {
            self.log(format!("Failed to save imported covenant: {e}"));
            return;
        }
        if let Err(e) = self.db.put_prover_key(covenant_id, &secret_key.secret_bytes()) {
            self.log(format!("Failed to save prover key: {e}"));
            return;
        }

        self.log(format!("Imported covenant {covenant_id} — prover address: {prover_addr}"));

        // Refresh list and select the imported covenant
        self.covenants = self.db.list_covenants();
        if let Some(idx) = self.covenants.iter().position(|(id, _)| *id == covenant_id) {
            self.covenant_list_index = idx;
            self.selected_covenant = Some(idx);
            self.load_prover_key(covenant_id);
            self.refresh_accounts();
            self.subscribe_covenant_addresses();

            // Auto-init prover (deployed covenant)
            let initial_state_root = zk_covenant_rollup_core::state::empty_tree_root();
            let initial_seq = zk_covenant_rollup_host::mock_chain::calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
            self.prover = Some(RollupProver::new(covenant_id, initial_state_root, initial_seq, self.pruning_point));
            self.log("Auto-initialized prover for imported covenant".into());
            self.pending_ops.push(PendingOp::FetchAndProcessChain);
        }
    }

    fn copy_covenant_info(&mut self) {
        if self.covenants.is_empty() {
            self.log("No covenants to copy".into());
            return;
        }
        let (id, ref record) = self.covenants[self.covenant_list_index];
        let mut info = format!("Covenant: {id}");
        if let Some(tx_id) = record.deployment_tx_id {
            info.push_str(&format!("\nDeploy TX: {tx_id}"));
        }
        let _ = cli_clipboard::set_contents(info);
        self.log("Covenant ID + Deploy TX copied to clipboard".into());
    }

    fn load_prover_key(&mut self, covenant_id: CovenantId) {
        self.prover_key = match self.db.get_prover_key(covenant_id) {
            Ok(Some(privkey)) => {
                let sk = match secp256k1::SecretKey::from_slice(&privkey) {
                    Ok(sk) => sk,
                    Err(_) => return,
                };
                let pk = sk.public_key(secp256k1::SECP256K1);
                let (xonly, _) = pk.x_only_public_key();
                let pubkey = Hash::from_bytes(xonly.serialize());
                Some((pubkey, privkey))
            }
            _ => None,
        };
    }

    /// Get the prover address as a string.
    pub fn prover_address(&self) -> Option<String> {
        let (pk, _) = self.prover_key.as_ref()?;
        let addr = Address::new(self.network_prefix, Version::PubKey, &pk.as_bytes());
        Some(addr.to_string())
    }

    // ── Proving tab ──

    fn handle_proving_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char('p') => self.start_chain_processing(),
            KeyCode::Char('b') => {
                // Cycle prover backend: CPU -> CUDA -> IPC -> CPU
                self.prover_backend = self.prover_backend.next();
                self.log(format!("Prover backend: {}", self.prover_backend.label()));
            }
            KeyCode::Char('k') => {
                // Cycle proof kind
                self.proof_kind = match self.proof_kind {
                    ProofKind::Succinct => ProofKind::Groth16,
                    ProofKind::Groth16 => ProofKind::Succinct,
                };
                self.log(format!("Proof kind: {}", self.proof_kind.label()));
            }
            KeyCode::Char('r') => self.start_proving(),
            KeyCode::Esc => self.cancel_proving(),
            _ => {}
        }
    }

    fn cancel_proving(&mut self) {
        if !self.proof_in_progress {
            return;
        }
        // Bump generation so the stale result is discarded when it arrives
        self.proof_generation += 1;
        self.proof_in_progress = false;
        self.last_proof_result = Some("Proof cancelled by user".into());
        self.log("Proof cancelled (background thread will finish eventually)".into());
    }

    fn start_proving(&mut self) {
        if self.proof_in_progress {
            self.log("Proof already in progress".into());
            return;
        }

        let prover = match &mut self.prover {
            Some(p) => p,
            None => {
                self.log("No prover initialized — select a deployed covenant first".into());
                return;
            }
        };

        let accumulated = prover.accumulated_blocks();
        if accumulated == 0 {
            self.log("No blocks accumulated to prove — wait for chain sync".into());
            return;
        }

        let input = match prover.take_prove_snapshot() {
            Some(input) => input,
            None => {
                self.log("Failed to create proving snapshot".into());
                return;
            }
        };

        self.proof_generation += 1;
        self.proof_in_progress = true;
        self.last_proof_result = None;
        let gen = self.proof_generation;
        self.log(format!(
            "Starting proof: {} blocks, backend={}, kind={}",
            input.block_txs.len(),
            self.prover_backend.label(),
            self.proof_kind.label()
        ));

        self.pending_ops.push(PendingOp::GenerateProof { gen, input, backend: self.prover_backend, kind: self.proof_kind });
    }

    pub fn start_chain_processing(&mut self) {
        if self.selected_covenant.is_none() {
            self.log("Select a covenant first".into());
            return;
        }

        // Initialize prover if not already done
        if self.prover.is_none() {
            let cov_idx = self.selected_covenant.unwrap();
            let covenant_id = self.covenants[cov_idx].0;

            let initial_state_root = zk_covenant_rollup_core::state::empty_tree_root();
            let initial_seq = zk_covenant_rollup_host::mock_chain::calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());

            self.prover = Some(RollupProver::new(covenant_id, initial_state_root, initial_seq, self.pruning_point));
            self.log("Initialized prover with empty state".into());
        }

        self.proving_status = "Fetching chain data...".into();
        self.pending_ops.push(PendingOp::FetchAndProcessChain);
    }

    // ── Node events ──

    fn handle_node_event(&mut self, event: NodeEvent) {
        match event {
            NodeEvent::Connected => {
                self.connected = true;
                self.log("Connected to Kaspa node".into());
            }
            NodeEvent::Disconnected => {
                self.connected = false;
                self.log("Disconnected from Kaspa node".into());
            }
            NodeEvent::Notification(notification) => {
                self.handle_notification(notification);
            }
        }
    }

    fn handle_notification(&mut self, notification: Notification) {
        match notification {
            Notification::VirtualDaaScoreChanged(n) => {
                self.daa_score = n.virtual_daa_score;
            }
            Notification::UtxosChanged(n) => {
                self.utxo_tracker.apply_utxos_changed(&n.added, &n.removed);
            }
            _ => {}
        }
    }

    // ── Background task dispatch ──

    /// Dispatch all queued pending operations to background tokio tasks.
    /// This is non-blocking — it spawns tasks and returns immediately.
    fn dispatch_pending_ops(&mut self) {
        let ops: Vec<PendingOp> = self.pending_ops.drain(..).collect();
        for op in ops {
            match op {
                PendingOp::SubscribeAndFetchUtxos(addresses) => {
                    let node = self.node.clone();
                    let tx = self.bg_tx.clone();
                    let address_count = addresses.len();
                    tokio::spawn(async move {
                        match node.get_utxos_by_addresses(addresses.clone()).await {
                            Ok(entries) => {
                                let _ = tx.send(BgResult::UtxosFetched { entries, address_count });
                            }
                            Err(e) => {
                                let _ = tx.send(BgResult::UtxosFetchFailed(e.to_string()));
                            }
                        }
                        if let Err(e) = node.subscribe_utxos(addresses).await {
                            let _ = tx.send(BgResult::UtxoSubscribeFailed(e.to_string()));
                        }
                    });
                }
                PendingOp::FetchAndProcessChain => {
                    if self.chain_sync_active {
                        continue; // don't double-fire
                    }
                    if let Some(prover) = &self.prover {
                        self.chain_sync_active = true;
                        let node = self.node.clone();
                        let start_hash = prover.last_processed_block;
                        let tx = self.bg_tx.clone();
                        tokio::spawn(async move {
                            match node.get_virtual_chain_v2(start_hash, Some(100)).await {
                                Ok(resp) => {
                                    let _ = tx.send(BgResult::ChainFetched(resp));
                                }
                                Err(e) => {
                                    let _ = tx.send(BgResult::ChainFetchFailed(e.to_string()));
                                }
                            }
                        });
                    }
                }
                PendingOp::SubmitTransaction(transaction) => {
                    let tx_id = transaction.id();
                    // Mark inputs spent immediately (before background submission)
                    for input in &transaction.inputs {
                        self.utxo_tracker.mark_spent(input.previous_outpoint.transaction_id, input.previous_outpoint.index);
                    }
                    let rpc_tx = tx_to_rpc(transaction);
                    let node = self.node.clone();
                    let tx = self.bg_tx.clone();
                    tokio::spawn(async move {
                        match node.submit_transaction(rpc_tx, false).await {
                            Ok(_) => {
                                let _ = tx.send(BgResult::TxSubmitted { tx_id });
                            }
                            Err(e) => {
                                let _ = tx.send(BgResult::TxSubmitFailed { tx_id, error: e.to_string() });
                            }
                        }
                    });
                }
                PendingOp::BuildAndSubmitAction { action, amount } => {
                    self.spawn_build_action(action, amount);
                }
                PendingOp::GenerateProof { gen, input, backend, kind } => {
                    let bg_tx = self.bg_tx.clone();
                    tokio::task::spawn_blocking(move || match host_prove::prove(&input, backend, kind) {
                        Ok(output) => {
                            let _ = bg_tx.send(BgResult::ProofCompleted {
                                gen,
                                elapsed_ms: output.elapsed_ms,
                                segments: output.stats.segments,
                                total_cycles: output.stats.total_cycles,
                            });
                        }
                        Err(e) => {
                            let _ = bg_tx.send(BgResult::ProofFailed { gen, error: e });
                        }
                    });
                }
            }
        }
    }

    /// Gather data from App state synchronously, then spawn the CPU-bound nonce
    /// grinding on a blocking thread.
    fn spawn_build_action(&mut self, action: ActionType, amount: u64) {
        let cov_idx = match self.selected_covenant {
            Some(i) => i,
            None => {
                self.log("No covenant selected".into());
                self.input_mode = InputMode::Normal;
                return;
            }
        };
        let covenant_id = self.covenants[cov_idx].0;
        let network_prefix = self.network_prefix;
        let accounts = self.accounts.clone();
        let account_list_index = self.account_list_index;

        // Select gas UTXO synchronously from current tracker state
        let gas_utxo = match action {
            ActionType::Entry => {
                let (dest_pk, _) = accounts[account_list_index];
                let dest_addr_str = self.pubkey_to_address(&dest_pk).unwrap_or_default();
                let utxos = self.utxo_tracker.available_utxos(&dest_addr_str);
                utxos.first().map(|u| (*u).clone())
            }
            ActionType::Transfer => {
                let (source_pk, _) = accounts[0];
                let source_addr = self.pubkey_to_address(&source_pk).unwrap_or_default();
                let utxos = self.utxo_tracker.available_utxos(&source_addr);
                utxos.first().map(|u| (*u).clone())
            }
            ActionType::Exit => {
                let (source_pk, _) = accounts[account_list_index];
                let source_addr = self.pubkey_to_address(&source_pk).unwrap_or_default();
                let utxos = self.utxo_tracker.available_utxos(&source_addr);
                utxos.first().map(|u| (*u).clone())
            }
        };

        let gas_utxo = match gas_utxo {
            Some(u) => u,
            None => {
                self.log(format!("No UTXOs available for {}", action.label()));
                self.input_mode = InputMode::Normal;
                return;
            }
        };

        let bg_tx = self.bg_tx.clone();

        // Nonce grinding is CPU-bound — run on blocking thread pool
        tokio::task::spawn_blocking(move || {
            let result = match action {
                ActionType::Entry => {
                    let (dest_pk, _) = accounts[account_list_index];
                    if amount > gas_utxo.amount {
                        Err(format!("Deposit {} exceeds UTXO value {}", amount, gas_utxo.amount))
                    } else {
                        Ok(crate::actions::build_entry_tx(dest_pk, covenant_id, amount, &gas_utxo))
                    }
                }
                ActionType::Transfer => {
                    if accounts.len() < 2 {
                        Err("Need at least 2 accounts".into())
                    } else {
                        let (source_pk, _) = accounts[0];
                        let (dest_pk, _) = accounts[1];
                        let dest_addr = Address::new(network_prefix, Version::PubKey, &dest_pk.as_bytes());
                        Ok(crate::actions::build_transfer_tx(source_pk, dest_pk, amount, &gas_utxo, &dest_addr))
                    }
                }
                ActionType::Exit => {
                    let (source_pk, _) = accounts[account_list_index];
                    let dest_spk = pay_to_address_script(&Address::new(network_prefix, Version::PubKey, &source_pk.as_bytes()));
                    Ok(crate::actions::build_exit_tx(source_pk, amount, dest_spk.script(), &gas_utxo))
                }
            };

            match result {
                Ok(tx) => {
                    let _ = bg_tx.send(BgResult::ActionBuilt { action, amount, tx });
                }
                Err(e) => {
                    let _ = bg_tx.send(BgResult::ActionBuildFailed { action, error: e });
                }
            }
        });
    }

    // ── Background result handling ──

    /// Process a result delivered from a background task.
    fn handle_bg_result(&mut self, result: BgResult) {
        match result {
            BgResult::UtxosFetched { entries, address_count } => {
                self.utxo_tracker.clear();
                self.utxo_tracker.load_initial(&entries);
                self.log(format!("Loaded {} UTXOs for {} addresses", entries.len(), address_count));
            }
            BgResult::UtxosFetchFailed(e) => {
                self.log(format!("Failed to fetch UTXOs: {e}"));
            }
            BgResult::UtxoSubscribeFailed(e) => {
                self.log(format!("Failed to subscribe UTXOs: {e}"));
            }
            BgResult::ChainFetched(response) => {
                self.chain_sync_active = false;
                if let Some(prover) = &mut self.prover {
                    let result = prover.process_chain_response(&response);
                    let root_hex = faster_hex::hex_string(bytemuck::bytes_of(&result.new_state_root));
                    self.proving_status = format!(
                        "Processed {} blocks, {} txs, {} actions | root: {}..{}",
                        result.blocks_processed,
                        result.txs_processed,
                        result.actions_found,
                        &root_hex[..8],
                        &root_hex[root_hex.len() - 8..],
                    );
                    if result.blocks_processed > 0 {
                        self.log(format!(
                            "Chain processing: {} blocks, {} txs ({} actions)",
                            result.blocks_processed, result.txs_processed, result.actions_found,
                        ));
                    }
                }
            }
            BgResult::ChainFetchFailed(e) => {
                self.chain_sync_active = false;
                self.proving_status = format!("Error: {e}");
                self.log(format!("Failed to fetch chain data: {e}"));
            }
            BgResult::TxSubmitted { tx_id } => {
                self.log(format!("Submitted tx: {tx_id}"));
                let _ = cli_clipboard::set_contents(tx_id.to_string());
                self.log("Tx ID copied to clipboard".into());
            }
            BgResult::TxSubmitFailed { tx_id, error } => {
                self.log(format!("Failed to submit tx {tx_id}: {error}"));
            }
            BgResult::ActionBuilt { action, amount, tx } => {
                let tx_id = tx.id();
                for input in &tx.inputs {
                    self.utxo_tracker.mark_spent(input.previous_outpoint.transaction_id, input.previous_outpoint.index);
                }
                self.record_tx(tx_id, action.label(), amount);
                self.log(format!("{} tx built: {tx_id}", action.label()));
                self.pending_ops.push(PendingOp::SubmitTransaction(tx));
                self.input_mode = InputMode::Normal;
            }
            BgResult::ActionBuildFailed { action, error } => {
                self.log(format!("Failed to build {} tx: {error}", action.label()));
                self.input_mode = InputMode::Normal;
            }
            BgResult::ProofCompleted { gen, elapsed_ms, segments, total_cycles } => {
                if gen != self.proof_generation {
                    self.log("Discarded stale proof result (cancelled)".to_string());
                    return;
                }
                self.proof_in_progress = false;
                let result_msg =
                    format!("Proof completed in {:.1}s ({} segments, {} cycles)", elapsed_ms as f64 / 1000.0, segments, total_cycles);
                self.log(result_msg.clone());
                self.last_proof_result = Some(result_msg);

                // Save proving state to DB
                if let Some(prover) = &self.prover {
                    if let Some(cov_idx) = self.selected_covenant {
                        let covenant_id = self.covenants[cov_idx].0;
                        let state = ProvingState {
                            last_proved_block_hash: prover.last_processed_block,
                            state_root: Hash::from_bytes(bytemuck::cast(prover.state_root)),
                            seq_commitment: prover.seq_commitment,
                            proof_count: self.db.get_proving_state(covenant_id).ok().flatten().map(|s| s.proof_count + 1).unwrap_or(1),
                        };
                        if let Err(e) = self.db.put_proving_state(covenant_id, &state) {
                            self.log(format!("Failed to save proving state: {e}"));
                        } else {
                            self.log(format!("Proving state saved (proof #{})", state.proof_count));
                        }
                    }
                }
            }
            BgResult::ProofFailed { gen, error } => {
                if gen != self.proof_generation {
                    self.log("Discarded stale proof failure (cancelled)".to_string());
                    return;
                }
                self.proof_in_progress = false;
                let result_msg = format!("Proof failed: {error}");
                self.log(result_msg.clone());
                self.last_proof_result = Some(result_msg);
            }
        }
    }

    // ── Tx History ──

    fn handle_tx_history_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.tx_history_index > 0 {
                    self.tx_history_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.tx_history.is_empty() && self.tx_history_index < self.tx_history.len() - 1 {
                    self.tx_history_index += 1;
                }
            }
            KeyCode::Char('c') => {
                // Copy selected tx ID to clipboard
                if let Some(record) = self.tx_history.get(self.tx_history_index) {
                    let _ = cli_clipboard::set_contents(record.tx_id.to_string());
                    self.log("Tx ID copied to clipboard".into());
                }
            }
            KeyCode::Enter | KeyCode::Char('o') => {
                // Open in browser
                if let Some(record) = self.tx_history.get(self.tx_history_index) {
                    let url = format!("https://tn12.kaspa.stream/transactions/{}", record.tx_id);
                    if let Err(e) = open::that(&url) {
                        self.log(format!("Failed to open browser: {e}"));
                    }
                }
            }
            _ => {}
        }
    }

    fn record_tx(&mut self, tx_id: Hash, action: &str, amount: u64) {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        self.tx_history.push(TxRecord { tx_id, action: action.to_string(), amount, timestamp, status: TxStatus::Submitted });
        self.tx_history_index = self.tx_history.len().saturating_sub(1);
    }

    // ── Helpers ──

    /// Auto-select the first covenant if any exist.
    /// Called once after connection to bootstrap the prover + subscriptions.
    pub fn auto_select_first_covenant(&mut self) {
        if self.covenants.is_empty() || self.selected_covenant.is_some() {
            return;
        }
        self.covenant_list_index = 0;
        self.selected_covenant = Some(0);
        let id = self.covenants[0].0;
        let is_deployed = self.covenants[0].1.deployment_tx_id.is_some();
        self.refresh_accounts();
        self.load_prover_key(id);
        self.subscribe_covenant_addresses();
        self.log(format!("Auto-selected covenant: {id}"));

        if is_deployed && self.prover.is_none() {
            let initial_state_root = zk_covenant_rollup_core::state::empty_tree_root();
            let initial_seq = zk_covenant_rollup_host::mock_chain::calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
            self.prover = Some(RollupProver::new(id, initial_state_root, initial_seq, self.pruning_point));
            self.log("Auto-initialized prover for deployed covenant".into());
            self.pending_ops.push(PendingOp::FetchAndProcessChain);
        }
    }

    pub fn log(&mut self, msg: String) {
        self.log_messages.push(msg);
    }

    /// Get the deployer address for a covenant record.
    pub fn deployer_address(&self, record: &CovenantRecord) -> Option<String> {
        if record.deployer_privkey.len() != 32 {
            return None;
        }
        let sk = secp256k1::SecretKey::from_slice(&record.deployer_privkey).ok()?;
        let pk = sk.public_key(secp256k1::SECP256K1);
        let (xonly, _) = pk.x_only_public_key();
        let addr = Address::new(self.network_prefix, Version::PubKey, &xonly.serialize());
        Some(addr.to_string())
    }

    /// Derive a Kaspa address from an x-only public key (stored as Hash).
    pub fn pubkey_to_address(&self, pubkey: &Pubkey) -> Option<String> {
        let addr = Address::new(self.network_prefix, Version::PubKey, &pubkey.as_bytes());
        Some(addr.to_string())
    }

    /// Get the deployer Address object for a covenant record.
    fn deployer_address_obj(&self, record: &CovenantRecord) -> Option<Address> {
        if record.deployer_privkey.len() != 32 {
            return None;
        }
        let sk = secp256k1::SecretKey::from_slice(&record.deployer_privkey).ok()?;
        let pk = sk.public_key(secp256k1::SECP256K1);
        let (xonly, _) = pk.x_only_public_key();
        Some(Address::new(self.network_prefix, Version::PubKey, &xonly.serialize()))
    }
}

/// Convert a consensus `Transaction` to an `RpcTransaction` for submission.
fn tx_to_rpc(tx: Transaction) -> RpcTransaction {
    RpcTransaction {
        version: tx.version,
        inputs: tx.inputs.into_iter().map(Into::into).collect(),
        outputs: tx.outputs.into_iter().map(Into::into).collect(),
        lock_time: tx.lock_time,
        subnetwork_id: tx.subnetwork_id,
        gas: tx.gas,
        payload: tx.payload,
        mass: 0,
        verbose_data: None,
    }
}
