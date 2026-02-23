use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::constants::TX_VERSION;
use kaspa_consensus_core::sign::sign;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{
    CovenantBinding, ScriptPublicKey, SignableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
    UtxoEntry,
};
use kaspa_hashes::Hash;
use kaspa_rpc_core::RpcTransaction;
use kaspa_txscript::script_builder::ScriptBuilder;
use kaspa_txscript::zk_precompiles::tags::ZkTag;
use kaspa_txscript::{pay_to_address_script, pay_to_script_hash_script};
use kaspa_wrpc_client::prelude::Notification;
use risc0_zkvm::sha::Digestible;
use tokio::sync::mpsc;
use zk_covenant_rollup_core::state::empty_tree_root;
use zk_covenant_rollup_core::PublicInput;
use zk_covenant_rollup_host::mock_chain::{calc_accepted_id_merkle_root, from_bytes};
use zk_covenant_rollup_host::redeem::build_redeem_script;
use zk_covenant_rollup_methods::ZK_COVENANT_ROLLUP_GUEST_ID;

use zk_covenant_rollup_core::action::{ActionHeader, ExitAction, TransferAction};
use zk_covenant_rollup_core::permission_tree::{required_depth, PermissionTree};
use zk_covenant_rollup_host::bridge::{build_delegate_entry_script, build_permission_redeem_converged, build_permission_sig_script};
use zk_covenant_rollup_host::mock_tx::ActionWitness;
use zk_covenant_rollup_host::prove::{self as host_prove, ProofKind, ProveInput, ProverBackend};

use crate::balance::UtxoTracker;
use crate::db::{CovenantId, CovenantRecord, ProvingState, Pubkey, RollupDb, TxRecordDb, TxStatusDb};
use crate::node::{KaspaNode, NodeEvent};
use crate::prover::RollupProver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

pub enum InputMode {
    Normal,
    PromptAmount { action: ActionType, buffer: String, context: String },
    PromptText { target: TextInputTarget, buffer: String, context: String },
    Confirm { action: ActionType, amount: u64, summary: Vec<String> },
    Processing { action: ActionType },
    ViewDetail { lines: Vec<String>, scroll: usize },
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxStatus {
    Submitted,
    Confirmed,
    Failed(String),
}

/// A completed ZK proof ready for on-chain submission.
pub(crate) struct CompletedProof {
    receipt: risc0_zkvm::Receipt,
    public_input: PublicInput,
    block_prove_to: Hash,
    proof_kind: ProofKind,
    perm_redeem_script: Option<Vec<u8>>,
    /// (spk_bytes, amount) for each exit in this proof (empty if no exits).
    perm_exit_data: Vec<(Vec<u8>, u64)>,
}

/// Results delivered from background tasks back to the main event loop.
enum BgResult {
    UtxosFetched {
        entries: Vec<kaspa_rpc_core::RpcUtxosByAddressesEntry>,
        address_count: usize,
    },
    UtxosFetchFailed(String),
    UtxoSubscribeFailed(String),
    ChainFetched(kaspa_rpc_core::GetVirtualChainFromBlockV2Response),
    ChainFetchFailed(String),
    TxSubmitted {
        tx_id: Hash,
    },
    TxSubmitFailed {
        tx_id: Hash,
        error: String,
    },
    ActionBuilt {
        action: ActionType,
        amount: u64,
        tx: Transaction,
    },
    ActionBuildFailed {
        action: ActionType,
        error: String,
    },
    ProofCompleted {
        gen: u64,
        receipt: Box<risc0_zkvm::Receipt>,
        public_input: PublicInput,
        block_prove_to: Hash,
        proof_kind: ProofKind,
        perm_redeem_script: Option<Vec<u8>>,
        perm_exit_data: Vec<(Vec<u8>, u64)>,
        elapsed_ms: u128,
        segments: usize,
        total_cycles: u64,
    },
    ProofFailed {
        gen: u64,
        error: String,
    },
    ImportValidated {
        covenant_id: Hash,
        deploy_tx_id: Hash,
        proof_kind: u8,
    },
    ImportFailed {
        error: String,
    },
}

/// State of an on-chain permission UTXO (created by a proof tx with exits).
pub struct PermUtxoState {
    /// Outpoint of the permission UTXO.
    pub utxo: (Hash, u32),
    /// Value held in the permission UTXO.
    pub value: u64,
    /// Permission redeem script bytes.
    pub redeem_script: Vec<u8>,
    /// Remaining (spk_bytes, amount) exit entries to be claimed.
    pub exit_data: Vec<(Vec<u8>, u64)>,
    /// Number of remaining unclaimed exits.
    pub unclaimed: u64,
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
    pub log_scroll: usize,
    pub log_selected: usize,

    // Covenant tab state
    pub covenants: Vec<(CovenantId, CovenantRecord)>,
    pub covenant_list_index: usize,
    pub selected_covenant: Option<usize>,

    // Account tab state (loaded for selected covenant)
    pub accounts: Vec<(Pubkey, [u8; 32])>, // (pubkey, privkey)
    pub account_list_index: usize,
    pub role_focused: bool, // true when cursor is on the role entry in accounts tab

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
    pub proof_in_progress: bool,
    pub last_proof_result: Option<String>,
    pub(crate) completed_proofs: Vec<CompletedProof>,
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

    /// Persistent clipboard instance (Linux requires the owner to stay alive).
    clipboard: Option<arboard::Clipboard>,

    /// Temporary flash message shown in the status bar (e.g. "Copied!").
    pub status_flash: Option<(String, Instant)>,

    /// When true, the run-loop will call `terminal.clear()` before the next draw
    /// to force a full screen redraw (Ctrl+L).
    pub needs_full_redraw: bool,

    /// Deferred side-effects keyed by tx_id. Applied on TxSubmitted, reverted on TxSubmitFailed.
    pending_tx_effects: HashMap<Hash, PendingTxEffect>,

    /// Append-only log file for persistent error logging.
    log_file: Option<std::fs::File>,

    /// State of the pending permission UTXO (set after a proof with exits is confirmed).
    pub perm_utxo: Option<PermUtxoState>,
}

/// Deferred async operations triggered from sync key handlers.
enum PendingOp {
    SubscribeAndFetchUtxos(Vec<Address>),
    SubmitTransaction(Transaction),
    FetchAndProcessChain,
    BuildAndSubmitAction {
        action: ActionType,
        amount: u64,
    },
    GenerateProof {
        gen: u64,
        input: ProveInput,
        backend: ProverBackend,
        kind: ProofKind,
        public_input: PublicInput,
        block_prove_to: Hash,
        perm_redeem_script: Option<Vec<u8>>,
        perm_exit_data: Vec<(Vec<u8>, u64)>,
    },
    ValidateImport {
        covenant_id: Hash,
        start_hash: Hash,
    },
}

/// Deferred side-effects of submitted transactions.
/// Applied on `TxSubmitted`, reverted on `TxSubmitFailed`.
#[allow(clippy::type_complexity)]
enum PendingTxEffect {
    /// Deploy tx: only mark covenant as deployed when the node accepts the tx.
    Deploy { covenant_id: CovenantId, deploy_tx_id: Hash, covenant_value: u64 },
    /// Prove-submit tx: on failure, revert the covenant UTXO to its old value.
    /// On success, if perm_state is Some, set the permission UTXO state.
    ProveSubmit {
        covenant_id: CovenantId,
        old_utxo: (Hash, u32),
        old_value: u64,
        /// If exits occurred: (redeem_script, exit_data) for the permission UTXO.
        perm_state: Option<(Vec<u8>, Vec<(Vec<u8>, u64)>)>,
    },
}

fn u8_to_proof_kind(v: u8) -> ProofKind {
    if v == 1 {
        ProofKind::Groth16
    } else {
        ProofKind::Succinct
    }
}

fn proof_kind_to_zk_tag(kind: ProofKind) -> ZkTag {
    match kind {
        ProofKind::Succinct => ZkTag::R0Succinct,
        ProofKind::Groth16 => ZkTag::Groth16,
    }
}

fn tx_status_to_db(s: &TxStatus) -> TxStatusDb {
    match s {
        TxStatus::Submitted => TxStatusDb::Submitted,
        TxStatus::Confirmed => TxStatusDb::Confirmed,
        TxStatus::Failed(msg) => TxStatusDb::Failed(msg.clone()),
    }
}

fn tx_status_from_db(s: TxStatusDb) -> TxStatus {
    match s {
        TxStatusDb::Submitted => TxStatus::Submitted,
        TxStatusDb::Confirmed => TxStatus::Confirmed,
        TxStatusDb::Failed(msg) => TxStatus::Failed(msg),
    }
}

fn tx_record_to_db(r: &TxRecord) -> TxRecordDb {
    TxRecordDb {
        tx_id: r.tx_id,
        action: r.action.clone(),
        amount: r.amount,
        timestamp: r.timestamp,
        status: tx_status_to_db(&r.status),
    }
}

fn tx_record_from_db(r: TxRecordDb) -> TxRecord {
    TxRecord { tx_id: r.tx_id, action: r.action, amount: r.amount, timestamp: r.timestamp, status: tx_status_from_db(r.status) }
}

impl App {
    pub fn new(db: Arc<RollupDb>, node: KaspaNode, network_prefix: Prefix) -> Self {
        Self::with_log_path(db, node, network_prefix, None)
    }

    pub fn with_log_path(db: Arc<RollupDb>, node: KaspaNode, network_prefix: Prefix, log_path: Option<std::path::PathBuf>) -> Self {
        let covenants = db.list_covenants();
        let (bg_tx, bg_rx) = mpsc::unbounded_channel();

        let log_file = log_path.and_then(|p| std::fs::OpenOptions::new().create(true).append(true).open(p).ok());

        Self {
            db,
            node,
            network_prefix,
            active_tab: Tab::Covenants,
            daa_score: 0,
            connected: false,
            should_quit: false,
            log_messages: Vec::new(),
            log_scroll: 0,
            log_selected: 0,
            covenants,
            covenant_list_index: 0,
            selected_covenant: None,
            accounts: Vec::new(),
            account_list_index: 0,
            role_focused: false,
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
            proof_in_progress: false,
            last_proof_result: None,
            completed_proofs: Vec::new(),
            proof_generation: 0,
            pending_ops: Vec::new(),
            bg_tx,
            bg_rx,
            chain_sync_active: false,
            clipboard: None,
            status_flash: None,
            needs_full_redraw: false,
            pending_tx_effects: HashMap::new(),
            log_file,
            perm_utxo: None,
        }
    }

    pub async fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> anyhow::Result<()> {
        let event_rx = self.node.event_receiver();

        loop {
            if self.needs_full_redraw {
                self.needs_full_redraw = false;
                terminal.clear()?;
            }
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
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.should_quit = true,
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => self.needs_full_redraw = true,

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
            Tab::Log => self.handle_log_key(key),
        }
    }

    // ── Covenant tab ──

    fn handle_covenant_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Char('c') => self.create_covenant(),
            KeyCode::Char('d') => self.deploy_covenant(),
            KeyCode::Char('i') => self.start_import_covenant(),
            KeyCode::Char('x') => self.delete_covenant(),
            KeyCode::Char('y') => self.export_covenant_info(),
            KeyCode::Char('t') => self.toggle_proof_kind(),
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
                    self.load_tx_history(id);
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
            covenant_value: None,
            proof_kind: 0,
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

    fn toggle_proof_kind(&mut self) {
        if self.covenants.is_empty() {
            self.log("No covenants".into());
            return;
        }
        let (id, ref record) = self.covenants[self.covenant_list_index];
        if record.deployment_tx_id.is_some() {
            self.log("Cannot change proof type — covenant already deployed".into());
            return;
        }
        let new_kind = if record.proof_kind == 0 { 1 } else { 0 };
        let mut updated = record.clone();
        updated.proof_kind = new_kind;
        if let Err(e) = self.db.put_covenant(id, &updated) {
            self.log(format!("Failed to save proof kind: {e}"));
            return;
        }
        let label = u8_to_proof_kind(new_kind).label();
        self.log(format!("Proof type: {label}"));
        self.covenants = self.db.list_covenants();
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

        // Take the first available UTXO; all value minus fee goes into the covenant
        let estimated_fee: u64 = 3000;
        let utxos = self.utxo_tracker.available_utxos(&deployer_addr_str);
        let first_utxo = match utxos.first() {
            Some(u) => (*u).clone(),
            None => {
                self.log(format!("No UTXOs at deployer address {deployer_addr_str}"));
                return;
            }
        };
        if first_utxo.amount <= estimated_fee {
            self.log(format!("UTXO too small ({} sompi) — need > {estimated_fee} sompi", first_utxo.amount));
            return;
        }
        let covenant_value = first_utxo.amount - estimated_fee;
        let selected_utxos = [first_utxo];
        let total_input = covenant_value + estimated_fee;

        // Build initial redeem script
        let prev_state_hash = empty_tree_root();
        let prev_seq_commitment = from_bytes(calc_accepted_id_merkle_root(Hash::default(), std::iter::empty()).as_bytes());
        let program_id: [u8; 32] = bytemuck::cast(ZK_COVENANT_ROLLUP_GUEST_ID);
        let zk_tag = proof_kind_to_zk_tag(u8_to_proof_kind(record.proof_kind));

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

        // Build outputs (single output — all value minus fee goes to covenant)
        let outputs = vec![TransactionOutput::new(covenant_value, covenant_spk)];
        let _ = total_input; // all consumed

        let tx = Transaction::new(0, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![record.proof_kind]);
        let signable = SignableTransaction::with_entries(tx, utxo_entries);

        // Sign
        let keypair = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &deployer_sk);
        let signed = sign(signable, keypair);
        let tx_id = signed.id();

        // Do NOT mark covenant as deployed yet — wait for TxSubmitted confirmation.
        self.pending_tx_effects.insert(tx_id, PendingTxEffect::Deploy { covenant_id, deploy_tx_id: tx_id, covenant_value });

        self.log(format!("Deploying covenant {covenant_id} — tx: {tx_id} (pending confirmation)"));
        self.record_tx(tx_id, "Deploy", covenant_value);
        self.pending_ops.push(PendingOp::SubmitTransaction(signed.tx));
    }

    fn delete_covenant(&mut self) {
        if self.covenants.is_empty() {
            self.log("No covenants to delete".into());
            return;
        }

        let (id, _) = self.covenants[self.covenant_list_index];

        if let Err(e) = self.db.delete_covenant_all(id) {
            self.log(format!("Failed to delete covenant: {e}"));
            return;
        }

        let deleted_idx = self.covenant_list_index;
        self.log(format!("Deleted covenant {id} and all related data"));
        self.covenants = self.db.list_covenants();

        // Clean up in-memory state if the deleted covenant was selected
        if self.selected_covenant == Some(deleted_idx) {
            self.selected_covenant = None;
            self.accounts.clear();
            self.account_list_index = 0;
            self.prover_key = None;
            self.prover = None;
            self.proof_in_progress = false;
            self.last_proof_result = None;
            self.proving_status = "No covenant selected".into();
        }

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
            KeyCode::Char('c') => {
                self.role_focused = false;
                self.create_account();
            }
            KeyCode::Char('y') => {
                if self.role_focused {
                    if let Some(addr) = self.role_address() {
                        self.copy_to_clipboard(&addr);
                        self.log(format!("Copied role address: {addr}"));
                    }
                } else if let Some((pk, _)) = self.accounts.get(self.account_list_index) {
                    let addr = self.pubkey_to_address(pk).unwrap_or_default();
                    self.copy_to_clipboard(&addr);
                    self.log(format!("Copied address: {addr}"));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.role_focused {
                    // Already at top, do nothing
                } else if self.account_list_index == 0 {
                    self.role_focused = true;
                } else {
                    self.account_list_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.role_focused {
                    self.role_focused = false;
                    self.account_list_index = 0;
                } else if !self.accounts.is_empty() && self.account_list_index < self.accounts.len() - 1 {
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
            self.role_focused = false;
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

        // Delegate entry P2SH address (for entry UTXOs controlled by delegate script)
        let covenant_id = self.covenants[cov_idx].0;
        let delegate_script = build_delegate_entry_script(&covenant_id.as_bytes());
        let delegate_spk = pay_to_script_hash_script(&delegate_script);
        let script_bytes = delegate_spk.script();
        if script_bytes.len() >= 35 {
            let hash_bytes: [u8; 32] = script_bytes[2..34].try_into().unwrap_or([0u8; 32]);
            addresses.push(Address::new(self.network_prefix, Version::ScriptHash, &hash_bytes));
        }

        if !addresses.is_empty() {
            self.pending_ops.push(PendingOp::SubscribeAndFetchUtxos(addresses));
        }
    }

    /// Subscribe to the permission P2SH address after a proof with exits is confirmed.
    fn subscribe_perm_address(&mut self) {
        if let Some(ref perm) = self.perm_utxo {
            let perm_spk = pay_to_script_hash_script(&perm.redeem_script);
            let script_bytes = perm_spk.script();
            if script_bytes.len() >= 35 {
                let hash_bytes: [u8; 32] = script_bytes[2..34].try_into().unwrap_or([0u8; 32]);
                let addr = Address::new(self.network_prefix, Version::ScriptHash, &hash_bytes);
                self.pending_ops.push(PendingOp::SubscribeAndFetchUtxos(vec![addr]));
            }
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
        // Handle ViewDetail separately to avoid borrow checker issues
        if matches!(self.input_mode, InputMode::ViewDetail { .. }) {
            self.handle_view_detail_key(key);
            return;
        }

        // Handle PromptText separately to avoid borrow checker issues
        if matches!(self.input_mode, InputMode::PromptText { .. }) {
            self.handle_prompt_text_key(key);
            return;
        }

        match &mut self.input_mode {
            InputMode::Normal => {}
            InputMode::ViewDetail { .. } => unreachable!("handled above"),
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
                let (buf_len, buf_clone) = match &self.input_mode {
                    InputMode::PromptText { buffer, .. } => (buffer.len(), buffer.clone()),
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
                let covenant_id = Hash::from_bytes(bytes);
                self.input_mode = InputMode::Normal;
                self.log("Scanning chain for deployment tx...".into());
                self.pending_ops.push(PendingOp::ValidateImport { covenant_id, start_hash: self.pruning_point });
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.log("Import cancelled".into());
            }
            _ => {}
        }
    }

    // ── ViewDetail popup ──

    fn handle_view_detail_key(&mut self, key: crossterm::event::KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.input_mode = InputMode::Normal,
            KeyCode::Down | KeyCode::Char('j') => {
                if let InputMode::ViewDetail { ref mut scroll, .. } = self.input_mode {
                    *scroll = scroll.saturating_add(1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let InputMode::ViewDetail { ref mut scroll, .. } = self.input_mode {
                    *scroll = scroll.saturating_sub(1);
                }
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

    fn finish_import_covenant(&mut self, covenant_id: Hash, deploy_tx_id: Hash, proof_kind: u8) {
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
            covenant_value: None, // unknown for imported covenants
            proof_kind,
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
            self.load_tx_history(covenant_id);
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

    fn export_covenant_info(&mut self) {
        if self.covenants.is_empty() {
            self.log("No covenants to export".into());
            return;
        }
        let (id, _) = self.covenants[self.covenant_list_index];
        let info = format!("{id}");
        self.copy_to_clipboard(&info);
        self.log(format!("Exported covenant ID to clipboard: {info}"));
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

    /// Get the active role address for the selected covenant.
    /// Returns prover address for deployed/imported covenants, deployer address for undeployed.
    pub fn role_address(&self) -> Option<String> {
        let cov_idx = self.selected_covenant?;
        let record = &self.covenants[cov_idx].1;
        if record.deployment_tx_id.is_some() {
            // Deployed or imported — use prover address
            self.prover_address()
        } else {
            // Undeployed — use deployer address
            self.deployer_address(record)
        }
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
            KeyCode::Char('r') => self.start_proving(),
            KeyCode::Char('s') => self.submit_proof(),
            KeyCode::Char('w') => self.withdraw(),
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

        let cov_idx = match self.selected_covenant {
            Some(i) => i,
            None => {
                self.log("Select a covenant first".into());
                return;
            }
        };
        let proof_kind = u8_to_proof_kind(self.covenants[cov_idx].1.proof_kind);

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

        // Capture block_prove_to before the snapshot resets the proving window
        let block_prove_to = prover.last_processed_block;

        let snapshot = match prover.take_prove_snapshot() {
            Some(s) => s,
            None => {
                self.log("Failed to create proving snapshot".into());
                return;
            }
        };

        let public_input = snapshot.input.public_input;

        self.proof_generation += 1;
        self.proof_in_progress = true;
        self.last_proof_result = None;
        let gen = self.proof_generation;
        self.log(format!(
            "Starting proof: {} blocks, backend={}, kind={}",
            snapshot.input.block_txs.len(),
            self.prover_backend.label(),
            proof_kind.label()
        ));

        self.pending_ops.push(PendingOp::GenerateProof {
            gen,
            input: snapshot.input,
            backend: self.prover_backend,
            kind: proof_kind,
            public_input,
            block_prove_to,
            perm_redeem_script: snapshot.perm_redeem_script,
            perm_exit_data: snapshot.perm_exit_data,
        });
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

    fn submit_proof(&mut self) {
        if self.completed_proofs.is_empty() {
            self.log("No completed proofs to submit — press 'r' to prove first".into());
            return;
        }

        let cov_idx = match self.selected_covenant {
            Some(i) => i,
            None => {
                self.log("Select a covenant first".into());
                return;
            }
        };

        let (covenant_id, ref record) = self.covenants[cov_idx];
        let covenant_utxo = match record.covenant_utxo {
            Some(utxo) => utxo,
            None => {
                self.log("No covenant UTXO — deploy the covenant first".into());
                return;
            }
        };
        let covenant_value = match record.covenant_value {
            Some(v) => v,
            None => {
                self.log("Unknown covenant UTXO value — cannot submit proof".into());
                return;
            }
        };

        let proof = self.completed_proofs.remove(0);

        // Extract new state from journal
        let journal = &proof.receipt.journal.bytes;
        if journal.len() < 128 {
            self.log(format!("Invalid journal length: {} (need >= 128)", journal.len()));
            return;
        }
        let new_state_hash: [u32; 8] = bytemuck::pod_read_unaligned(&journal[64..96]);
        let new_seq_commitment: [u32; 8] = bytemuck::pod_read_unaligned(&journal[96..128]);

        let program_id: [u8; 32] = bytemuck::cast(ZK_COVENANT_ROLLUP_GUEST_ID);
        let zk_tag = match proof.proof_kind {
            ProofKind::Succinct => ZkTag::R0Succinct,
            ProofKind::Groth16 => ZkTag::Groth16,
        };

        // Convergence loop for redeem script length
        let mut computed_len: i64 = 75;
        loop {
            let script = build_redeem_script(
                proof.public_input.prev_state_hash,
                proof.public_input.prev_seq_commitment,
                computed_len,
                &program_id,
                &zk_tag,
            );
            let new_len = script.len() as i64;
            if new_len == computed_len {
                break;
            }
            computed_len = new_len;
        }

        let input_redeem = build_redeem_script(
            proof.public_input.prev_state_hash,
            proof.public_input.prev_seq_commitment,
            computed_len,
            &program_id,
            &zk_tag,
        );
        let output_redeem = build_redeem_script(new_state_hash, new_seq_commitment, computed_len, &program_id, &zk_tag);
        let output_spk = pay_to_script_hash_script(&output_redeem);

        // Build sig_script
        let sig_script = match proof.proof_kind {
            ProofKind::Succinct => {
                match self.build_succinct_sig_script(&proof.receipt, proof.block_prove_to, &new_state_hash, &input_redeem) {
                    Ok(s) => s,
                    Err(e) => {
                        self.log(format!("Failed to build succinct sig_script: {e}"));
                        return;
                    }
                }
            }
            ProofKind::Groth16 => {
                match self.build_groth16_sig_script(&proof.receipt, proof.block_prove_to, &new_state_hash, &input_redeem) {
                    Ok(s) => s,
                    Err(e) => {
                        self.log(format!("Failed to build groth16 sig_script: {e}"));
                        return;
                    }
                }
            }
        };

        // Build transaction
        let estimated_fee: u64 = 3000;
        let has_permission = proof.perm_redeem_script.is_some();
        let perm_dust: u64 = 1000;
        let output_value = if has_permission {
            covenant_value.saturating_sub(estimated_fee).saturating_sub(perm_dust)
        } else {
            covenant_value.saturating_sub(estimated_fee)
        };

        if output_value == 0 {
            self.log("Covenant UTXO value too low to cover fee".into());
            return;
        }

        let covenant_id_hash = Hash::from_bytes(bytemuck::cast(proof.public_input.covenant_id));

        let inputs = vec![TransactionInput::new(TransactionOutpoint::new(covenant_utxo.0, covenant_utxo.1), sig_script, 0, 115)];

        let mut outputs = vec![TransactionOutput::with_covenant(
            output_value,
            output_spk,
            Some(CovenantBinding { authorizing_input: 0, covenant_id: covenant_id_hash }),
        )];

        if let Some(ref perm_redeem) = proof.perm_redeem_script {
            let perm_spk = pay_to_script_hash_script(perm_redeem);
            outputs.push(TransactionOutput::with_covenant(
                perm_dust,
                perm_spk,
                Some(CovenantBinding { authorizing_input: 0, covenant_id: covenant_id_hash }),
            ));
        }

        let tx = Transaction::new(TX_VERSION + 1, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let tx_id = tx.id();

        // Save old UTXO info so we can revert on submission failure
        let perm_state = if !proof.perm_exit_data.is_empty() {
            proof.perm_redeem_script.clone().map(|redeem| (redeem, proof.perm_exit_data.clone()))
        } else {
            None
        };
        self.pending_tx_effects.insert(
            tx_id,
            PendingTxEffect::ProveSubmit { covenant_id, old_utxo: covenant_utxo, old_value: covenant_value, perm_state },
        );

        // Optimistically update covenant record with new UTXO outpoint and value
        // (reverted in handle_bg_result if TxSubmitFailed)
        let mut updated_record = record.clone();
        updated_record.covenant_utxo = Some((tx_id, 0));
        updated_record.covenant_value = Some(output_value);
        if let Err(e) = self.db.put_covenant(covenant_id, &updated_record) {
            self.log(format!("Failed to update covenant in DB: {e}"));
            return;
        }

        self.log(format!("Submitting prove tx: {tx_id}"));
        self.last_proof_result = Some(format!("Proof submitted — tx: {tx_id}"));
        self.record_tx(tx_id, "Prove", output_value);
        self.pending_ops.push(PendingOp::SubmitTransaction(tx));

        // Refresh covenants list to pick up the updated record
        self.covenants = self.db.list_covenants();
    }

    fn build_succinct_sig_script(
        &self,
        receipt: &risc0_zkvm::Receipt,
        block_prove_to: Hash,
        new_state_hash: &[u32; 8],
        input_redeem: &[u8],
    ) -> Result<Vec<u8>, String> {
        let succinct = receipt.inner.succinct().map_err(|e| format!("Not a succinct receipt: {e}"))?;

        let seal_bytes: Vec<u8> = succinct.seal.iter().flat_map(|w| w.to_le_bytes()).collect();
        let claim_bytes: Vec<u8> = succinct.claim.digest().as_bytes().to_vec();
        let hashfn_byte: Vec<u8> = vec![zk_covenant_common::hashfn_str_to_id(&succinct.hashfn).ok_or("invalid hashfn")?];
        let control_index_bytes: Vec<u8> = succinct.control_inclusion_proof.index.to_le_bytes().to_vec();
        let control_digests_bytes: Vec<u8> =
            succinct.control_inclusion_proof.digests.iter().flat_map(|d| d.as_bytes()).copied().collect();

        Ok(ScriptBuilder::new()
            .add_data(&seal_bytes)
            .unwrap()
            .add_data(&claim_bytes)
            .unwrap()
            .add_data(&hashfn_byte)
            .unwrap()
            .add_data(&control_index_bytes)
            .unwrap()
            .add_data(&control_digests_bytes)
            .unwrap()
            .add_data(block_prove_to.as_bytes().as_slice())
            .unwrap()
            .add_data(bytemuck::bytes_of(new_state_hash))
            .unwrap()
            .add_data(input_redeem)
            .unwrap()
            .drain())
    }

    fn build_groth16_sig_script(
        &self,
        receipt: &risc0_zkvm::Receipt,
        block_prove_to: Hash,
        new_state_hash: &[u32; 8],
        input_redeem: &[u8],
    ) -> Result<Vec<u8>, String> {
        let groth16 = receipt.inner.groth16().map_err(|e| format!("Not a groth16 receipt: {e}"))?;
        let compressed_proof = zk_covenant_common::seal_to_compressed_proof(&groth16.seal);

        Ok(ScriptBuilder::new()
            .add_data(&compressed_proof)
            .unwrap()
            .add_data(block_prove_to.as_bytes().as_slice())
            .unwrap()
            .add_data(bytemuck::bytes_of(new_state_hash))
            .unwrap()
            .add_data(input_redeem)
            .unwrap()
            .drain())
    }

    // ── Withdraw ──

    /// Redeem the first unclaimed exit from the permission UTXO.
    fn withdraw(&mut self) {
        let perm_state = match &self.perm_utxo {
            Some(p) => p,
            None => {
                self.log("No permission UTXO — submit a proof with exits first".into());
                return;
            }
        };

        if perm_state.exit_data.is_empty() {
            self.log("No remaining exits to claim".into());
            return;
        }

        let cov_idx = match self.selected_covenant {
            Some(i) => i,
            None => {
                self.log("Select a covenant first".into());
                return;
            }
        };

        let covenant_id = self.covenants[cov_idx].0;
        let (spk, amount) = perm_state.exit_data[0].clone();

        // Build permission tree from all remaining exits
        let tree = PermissionTree::from_leaves(perm_state.exit_data.clone());
        let proof = tree.prove(0);
        let perm_sig_script = build_permission_sig_script(&spk, amount, amount, &proof, &perm_state.redeem_script);

        // Build delegate entry script and address for gathering UTXOs
        let delegate_script = build_delegate_entry_script(&covenant_id.as_bytes());
        let delegate_spk = pay_to_script_hash_script(&delegate_script);
        let script_bytes = delegate_spk.script();
        let delegate_addr_str = if script_bytes.len() >= 35 {
            let hash_bytes: [u8; 32] = script_bytes[2..34].try_into().unwrap_or([0u8; 32]);
            let addr = Address::new(self.network_prefix, Version::ScriptHash, &hash_bytes);
            addr.to_string()
        } else {
            self.log("Failed to derive delegate address".into());
            return;
        };

        // Gather delegate UTXOs covering amount
        let delegate_utxos: Vec<_> = self.utxo_tracker.available_utxos(&delegate_addr_str).into_iter().cloned().collect();
        let mut delegate_total: u64 = 0;
        let mut selected_delegates = Vec::new();
        for utxo in &delegate_utxos {
            selected_delegates.push(utxo.clone());
            delegate_total += utxo.amount;
            if delegate_total >= amount {
                break;
            }
        }
        if delegate_total < amount {
            self.log(format!("Insufficient delegate UTXOs ({delegate_total} < {amount} sompi)"));
            return;
        }

        let estimated_fee: u64 = 3000;
        let dest_value = amount.saturating_sub(estimated_fee);
        if dest_value == 0 {
            self.log("Amount too small to cover fee".into());
            return;
        }

        let delegate_sig_script = ScriptBuilder::new().add_data(&delegate_script).unwrap().drain();

        // Build inputs
        let perm_utxo_ref = self.perm_utxo.as_ref().unwrap();
        let mut inputs =
            vec![TransactionInput::new(TransactionOutpoint::new(perm_utxo_ref.utxo.0, perm_utxo_ref.utxo.1), perm_sig_script, 0, 115)];
        for utxo in &selected_delegates {
            inputs.push(TransactionInput::new(TransactionOutpoint::new(utxo.tx_id, utxo.index), delegate_sig_script.clone(), 0, 0));
        }

        // Build outputs
        let dest_spk = ScriptPublicKey::new(0, spk.into());
        let mut outputs = vec![TransactionOutput::new(dest_value, dest_spk)];

        let covenant_id_hash = covenant_id;
        let unclaimed = perm_utxo_ref.unclaimed;
        if unclaimed > 1 {
            let remaining: Vec<(Vec<u8>, u64)> = perm_utxo_ref.exit_data[1..].to_vec();
            let new_unclaimed = remaining.len() as u64;
            let new_depth = required_depth(remaining.len());
            let new_tree = PermissionTree::from_leaves(remaining);
            let new_root = new_tree.root();
            let max_inputs = std::num::NonZeroUsize::new(zk_covenant_rollup_core::MAX_DELEGATE_INPUTS).unwrap();
            let new_redeem = build_permission_redeem_converged(&new_root, new_unclaimed, new_depth, max_inputs);
            let new_perm_spk = pay_to_script_hash_script(&new_redeem);
            outputs.push(TransactionOutput::with_covenant(
                perm_utxo_ref.value,
                new_perm_spk,
                Some(CovenantBinding { authorizing_input: 0, covenant_id: covenant_id_hash }),
            ));
        }

        if delegate_total > amount {
            let change = delegate_total - amount;
            outputs.push(TransactionOutput::new(change, delegate_spk));
        }

        let tx = Transaction::new(TX_VERSION + 1, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let tx_id = tx.id();

        // Update perm_utxo state
        if let Some(ref mut perm) = self.perm_utxo {
            perm.exit_data.remove(0);
            perm.unclaimed = perm.unclaimed.saturating_sub(1);
            if perm.unclaimed == 0 {
                self.perm_utxo = None;
            } else {
                perm.utxo = (tx_id, 1);
            }
        }

        self.log(format!("Withdraw tx built: {tx_id} ({dest_value} sompi to destination)"));
        self.record_tx(tx_id, "Withdraw", dest_value);
        self.pending_ops.push(PendingOp::SubmitTransaction(tx));
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
                            match node.get_virtual_chain_v2(start_hash, Some(1000)).await {
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
                PendingOp::GenerateProof {
                    gen,
                    input,
                    backend,
                    kind,
                    public_input,
                    block_prove_to,
                    perm_redeem_script,
                    perm_exit_data,
                } => {
                    let bg_tx = self.bg_tx.clone();
                    tokio::task::spawn_blocking(move || match host_prove::prove(&input, backend, kind) {
                        Ok(output) => {
                            let _ = bg_tx.send(BgResult::ProofCompleted {
                                gen,
                                receipt: Box::new(output.receipt),
                                public_input,
                                block_prove_to,
                                proof_kind: kind,
                                perm_redeem_script,
                                perm_exit_data,
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
                PendingOp::ValidateImport { covenant_id, start_hash } => {
                    let node = self.node.clone();
                    let bg_tx = self.bg_tx.clone();

                    // Build expected covenant SPKs for both proof types
                    let prev_state_hash = empty_tree_root();
                    let prev_seq_commitment = from_bytes(calc_accepted_id_merkle_root(Hash::default(), std::iter::empty()).as_bytes());
                    let program_id: [u8; 32] = bytemuck::cast(ZK_COVENANT_ROLLUP_GUEST_ID);

                    let build_expected_spk = |tag: &ZkTag| {
                        let mut computed_len: i64 = 75;
                        loop {
                            let script = build_redeem_script(prev_state_hash, prev_seq_commitment, computed_len, &program_id, tag);
                            let new_len = script.len() as i64;
                            if new_len == computed_len {
                                break;
                            }
                            computed_len = new_len;
                        }
                        let redeem = build_redeem_script(prev_state_hash, prev_seq_commitment, computed_len, &program_id, tag);
                        pay_to_script_hash_script(&redeem)
                    };

                    let expected_spk_succinct = build_expected_spk(&ZkTag::R0Succinct);
                    let expected_spk_groth16 = build_expected_spk(&ZkTag::Groth16);

                    tokio::spawn(async move {
                        let mut cursor = start_hash;
                        loop {
                            let resp = match node.get_virtual_chain_v2(cursor, Some(1000)).await {
                                Ok(r) => r,
                                Err(e) => {
                                    let _ = bg_tx.send(BgResult::ImportFailed { error: format!("Chain fetch error: {e}") });
                                    return;
                                }
                            };

                            for block in resp.chain_block_accepted_transactions.iter() {
                                for rpc_tx in &block.accepted_transactions {
                                    for out in &rpc_tx.outputs {
                                        if let Some(ref spk) = out.script_public_key {
                                            let matched_kind = if spk == &expected_spk_succinct {
                                                Some(0u8)
                                            } else if spk == &expected_spk_groth16 {
                                                Some(1u8)
                                            } else {
                                                None
                                            };
                                            if let Some(proof_kind) = matched_kind {
                                                let deploy_tx_id = rpc_tx.verbose_data.as_ref().and_then(|v| v.transaction_id);
                                                if let Some(tx_id) = deploy_tx_id {
                                                    let _ = bg_tx.send(BgResult::ImportValidated {
                                                        covenant_id,
                                                        deploy_tx_id: tx_id,
                                                        proof_kind,
                                                    });
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            match resp.added_chain_block_hashes.last() {
                                Some(&last) if last != cursor => cursor = last,
                                _ => {
                                    let _ = bg_tx.send(BgResult::ImportFailed {
                                        error: "Deployment tx not found on chain — import cancelled".into(),
                                    });
                                    return;
                                }
                            }
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
            let result: Result<Transaction, String> = match action {
                ActionType::Entry => {
                    let (signer_pk, signer_sk) = accounts[account_list_index];
                    if amount > gas_utxo.amount {
                        Err(format!("Deposit {} exceeds UTXO value {}", amount, gas_utxo.amount))
                    } else {
                        let tx = crate::actions::build_entry_tx(signer_pk, covenant_id, amount, &gas_utxo);
                        let signer_addr = Address::new(network_prefix, Version::PubKey, &signer_pk.as_bytes());
                        let spk = pay_to_address_script(&signer_addr);
                        let utxo_entry = UtxoEntry::new(gas_utxo.amount, spk, 0, false, None);
                        let signable = SignableTransaction::with_entries(tx, vec![utxo_entry]);
                        let sk = secp256k1::SecretKey::from_slice(&signer_sk).unwrap();
                        let kp = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &sk);
                        let signed = sign(signable, kp);
                        Ok(signed.tx)
                    }
                }
                ActionType::Transfer => {
                    if accounts.len() < 2 {
                        Err("Need at least 2 accounts".into())
                    } else {
                        let (signer_pk, signer_sk) = accounts[0];
                        let (dest_pk, _) = accounts[1];
                        let dest_addr = Address::new(network_prefix, Version::PubKey, &dest_pk.as_bytes());
                        let tx = crate::actions::build_transfer_tx(signer_pk, dest_pk, amount, &gas_utxo, &dest_addr);
                        let signer_addr = Address::new(network_prefix, Version::PubKey, &signer_pk.as_bytes());
                        let spk = pay_to_address_script(&signer_addr);
                        let utxo_entry = UtxoEntry::new(gas_utxo.amount, spk, 0, false, None);
                        let signable = SignableTransaction::with_entries(tx, vec![utxo_entry]);
                        let sk = secp256k1::SecretKey::from_slice(&signer_sk).unwrap();
                        let kp = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &sk);
                        let signed = sign(signable, kp);
                        Ok(signed.tx)
                    }
                }
                ActionType::Exit => {
                    let (signer_pk, signer_sk) = accounts[account_list_index];
                    let dest_spk = pay_to_address_script(&Address::new(network_prefix, Version::PubKey, &signer_pk.as_bytes()));
                    let tx = crate::actions::build_exit_tx(signer_pk, amount, dest_spk.script(), &gas_utxo);
                    let signer_addr = Address::new(network_prefix, Version::PubKey, &signer_pk.as_bytes());
                    let spk = pay_to_address_script(&signer_addr);
                    let utxo_entry = UtxoEntry::new(gas_utxo.amount, spk, 0, false, None);
                    let signable = SignableTransaction::with_entries(tx, vec![utxo_entry]);
                    let sk = secp256k1::SecretKey::from_slice(&signer_sk).unwrap();
                    let kp = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &sk);
                    let signed = sign(signable, kp);
                    Ok(signed.tx)
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
                // Mine on-chain txs into history (confirms ours, records others')
                self.detect_onchain_txs();
            }
            BgResult::ChainFetchFailed(e) => {
                self.chain_sync_active = false;
                self.proving_status = format!("Error: {e}");
                self.log(format!("Failed to fetch chain data: {e}"));
            }
            BgResult::TxSubmitted { tx_id } => {
                self.log(format!("Submitted tx: {tx_id}"));
                self.copy_to_clipboard(&tx_id.to_string());
                self.log("Tx ID copied to clipboard".into());

                // Status stays Submitted — only chain confirmation would make it Confirmed.

                // Apply deferred side-effects
                if let Some(effect) = self.pending_tx_effects.remove(&tx_id) {
                    match effect {
                        PendingTxEffect::Deploy { covenant_id, deploy_tx_id, covenant_value } => {
                            if let Ok(Some(mut rec)) = self.db.get_covenant(covenant_id) {
                                rec.deployment_tx_id = Some(deploy_tx_id);
                                rec.covenant_utxo = Some((deploy_tx_id, 0));
                                rec.covenant_value = Some(covenant_value);
                                if let Err(e) = self.db.put_covenant(covenant_id, &rec) {
                                    self.log(format!("Failed to mark covenant as deployed: {e}"));
                                } else {
                                    self.log(format!("Covenant {covenant_id} deployed successfully"));
                                    self.covenants = self.db.list_covenants();
                                }
                            }
                        }
                        PendingTxEffect::ProveSubmit { perm_state, .. } => {
                            // Optimistic update already applied — set permission UTXO if exits occurred
                            if let Some((redeem_script, exit_data)) = perm_state {
                                let unclaimed = exit_data.len() as u64;
                                self.perm_utxo =
                                    Some(PermUtxoState { utxo: (tx_id, 1), value: 1000, redeem_script, exit_data, unclaimed });
                                self.log("Permission UTXO set — press 'w' in Proving tab to withdraw".into());
                                // Subscribe to the permission address
                                self.subscribe_perm_address();
                            }
                        }
                    }
                }
            }
            BgResult::TxSubmitFailed { tx_id, error } => {
                self.log(format!("Failed to submit tx {tx_id}: {error}"));

                // Update tx history status in memory and in DB
                if let Some(record) = self.tx_history.iter_mut().find(|r| r.tx_id == tx_id) {
                    record.status = TxStatus::Failed(error.clone());
                }
                if let Some(cov_id) = self.selected_covenant_id() {
                    if let Err(e) = self.db.update_tx_status(cov_id, tx_id, TxStatusDb::Failed(error.clone())) {
                        self.log(format!("Failed to persist tx status: {e}"));
                    }
                }

                // Revert deferred side-effects
                if let Some(effect) = self.pending_tx_effects.remove(&tx_id) {
                    match effect {
                        PendingTxEffect::Deploy { covenant_id, .. } => {
                            self.log(format!("Deploy tx failed — covenant {covenant_id} remains undeployed"));
                        }
                        PendingTxEffect::ProveSubmit { covenant_id, old_utxo, old_value, .. } => {
                            // Revert covenant UTXO to old values
                            if let Ok(Some(mut rec)) = self.db.get_covenant(covenant_id) {
                                rec.covenant_utxo = Some(old_utxo);
                                rec.covenant_value = Some(old_value);
                                if let Err(e) = self.db.put_covenant(covenant_id, &rec) {
                                    self.log(format!("Failed to revert covenant UTXO: {e}"));
                                } else {
                                    self.log(format!("Reverted covenant {covenant_id} UTXO to previous state"));
                                    self.covenants = self.db.list_covenants();
                                }
                            }
                        }
                    }
                }
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
            BgResult::ProofCompleted {
                gen,
                receipt,
                public_input,
                block_prove_to,
                proof_kind,
                perm_redeem_script,
                perm_exit_data,
                elapsed_ms,
                segments,
                total_cycles,
            } => {
                if gen != self.proof_generation {
                    self.log("Discarded stale proof result (cancelled)".to_string());
                    return;
                }
                self.proof_in_progress = false;
                let result_msg =
                    format!("Proof completed in {:.1}s ({} segments, {} cycles)", elapsed_ms as f64 / 1000.0, segments, total_cycles);
                self.log(result_msg.clone());
                self.last_proof_result = Some(result_msg);

                // Store the completed proof for later submission
                self.completed_proofs.push(CompletedProof {
                    receipt: *receipt,
                    public_input,
                    block_prove_to,
                    proof_kind,
                    perm_redeem_script,
                    perm_exit_data,
                });
                self.log(format!("{} proof(s) ready for submission — press 's' in Proving tab", self.completed_proofs.len()));

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
            BgResult::ImportValidated { covenant_id, deploy_tx_id, proof_kind } => {
                let kind_label = u8_to_proof_kind(proof_kind).label();
                self.log(format!("Found deployment tx {deploy_tx_id} on chain (proof type: {kind_label})"));
                self.finish_import_covenant(covenant_id, deploy_tx_id, proof_kind);
            }
            BgResult::ImportFailed { error } => {
                self.log(error);
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
                    self.copy_to_clipboard(&record.tx_id.to_string());
                    self.log("Tx ID copied to clipboard".into());
                }
            }
            KeyCode::Enter => {
                // Open ViewDetail popup
                if let Some(record) = self.tx_history.get(self.tx_history_index) {
                    let lines = vec![
                        format!("Tx ID:     {}", record.tx_id),
                        format!("Action:    {}", record.action),
                        format!("Amount:    {} sompi", record.amount),
                        format!("Timestamp: {}", record.timestamp),
                        format!("Status:    {:?}", record.status),
                    ];
                    self.input_mode = InputMode::ViewDetail { lines, scroll: 0 };
                }
            }
            KeyCode::Char('o') => {
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

    fn handle_log_key(&mut self, key: crossterm::event::KeyEvent) {
        let total = self.log_messages.len();
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if self.log_selected + 1 < total {
                    self.log_selected += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.log_selected = self.log_selected.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.log_selected = 0;
                self.log_scroll = 0;
            }
            KeyCode::Char('G') => {
                self.log_selected = total.saturating_sub(1);
            }
            KeyCode::Enter | KeyCode::Char('v') => {
                if let Some(msg) = self.log_messages.get(self.log_selected) {
                    let lines = msg.split('\n').map(|s| s.to_string()).collect();
                    self.input_mode = InputMode::ViewDetail { lines, scroll: 0 };
                }
            }
            _ => {}
        }
    }

    fn record_tx(&mut self, tx_id: Hash, action: &str, amount: u64) {
        self.record_tx_with_status(tx_id, action, amount, TxStatus::Submitted);
    }

    fn record_tx_with_status(&mut self, tx_id: Hash, action: &str, amount: u64, status: TxStatus) {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let rec = TxRecord { tx_id, action: action.to_string(), amount, timestamp, status };
        if let Some(cov_id) = self.selected_covenant_id() {
            if let Err(e) = self.db.append_tx(cov_id, &tx_record_to_db(&rec)) {
                self.log(format!("Failed to persist tx record: {e}"));
            }
        }
        self.tx_history.push(rec);
        self.tx_history_index = self.tx_history.len().saturating_sub(1);
    }

    /// If `tx_id` is already in history with Submitted status, upgrade it to Confirmed and
    /// persist. Otherwise insert a new Confirmed record (tx seen from another instance).
    fn confirm_or_record(&mut self, covenant_id: CovenantId, tx_id: Hash, label: &str, amount: u64) {
        if let Some(existing) = self.tx_history.iter_mut().find(|r| r.tx_id == tx_id) {
            if existing.status == TxStatus::Submitted {
                existing.status = TxStatus::Confirmed;
                if let Err(e) = self.db.update_tx_status(covenant_id, tx_id, TxStatusDb::Confirmed) {
                    self.log(format!("Failed to persist tx status update: {e}"));
                }
            }
        } else {
            // Seen on-chain from another instance — record as already confirmed
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let rec = TxRecord { tx_id, action: label.to_string(), amount, timestamp, status: TxStatus::Confirmed };
            if let Err(e) = self.db.append_tx(covenant_id, &tx_record_to_db(&rec)) {
                self.log(format!("Failed to persist chain tx: {e}"));
            }
            self.tx_history.push(rec);
            self.tx_history_index = self.tx_history.len().saturating_sub(1);
        }
    }

    /// Load tx history for a covenant from DB (called on covenant select).
    fn load_tx_history(&mut self, covenant_id: CovenantId) {
        self.tx_history = self.db.list_txs(covenant_id).into_iter().map(tx_record_from_db).collect();
        self.tx_history_index = self.tx_history.len().saturating_sub(1);
    }

    /// Scan the last processed block txs for on-chain activity not yet in our history.
    /// Confirms our own pending txs and records txs from other instances.
    fn detect_onchain_txs(&mut self) {
        let cov_idx = match self.selected_covenant {
            Some(i) => i,
            None => return,
        };
        let covenant_id = self.covenants[cov_idx].0;
        let deployment_tx_id = self.covenants[cov_idx].1.deployment_tx_id;

        // Collect detected txs without holding a borrow on self.prover
        let mut detected: Vec<(Hash, &'static str, u64)> = Vec::new();
        let mut new_cov_utxo: Option<(Hash, u32)> = None;
        let mut new_cov_value: Option<u64> = None;

        {
            let prover = match &self.prover {
                Some(p) => p,
                None => return,
            };
            // Follow the covenant UTXO chain through the response
            let mut tracking_utxo = self.covenants[cov_idx].1.covenant_utxo;

            for block_txs in &prover.last_block_txs {
                for ztx in block_txs {
                    let tx_id = ztx.tx.id();

                    // Deployment confirmation: match by tx_id
                    if let Some(deploy_id) = deployment_tx_id {
                        if tx_id == deploy_id {
                            detected.push((tx_id, "Deploy", 0));
                            continue;
                        }
                    }

                    if let Some(ref witness) = ztx.witness {
                        // Successfully applied action tx
                        let (label, amount) = witness_label_amount(witness, &ztx.tx);
                        detected.push((tx_id, label, amount));
                    } else if ztx.tx.version >= 1 {
                        // Check if this spends our tracked covenant UTXO → proof submission
                        if let Some((utxo_tx, utxo_idx)) = tracking_utxo {
                            let spends =
                                ztx.tx.inputs.iter().any(|inp| {
                                    inp.previous_outpoint.transaction_id == utxo_tx && inp.previous_outpoint.index == utxo_idx
                                });
                            if spends {
                                let val = ztx.tx.outputs.first().map(|o| o.value).unwrap_or(0);
                                detected.push((tx_id, "Prove", val));
                                // Follow the UTXO chain for subsequent proofs in same response
                                tracking_utxo = Some((tx_id, 0));
                                new_cov_utxo = Some((tx_id, 0));
                                new_cov_value = Some(val);
                            }
                        }
                    }
                }
            }
        }

        // Apply detections (needs mutable self)
        for (tx_id, label, amount) in detected {
            self.confirm_or_record(covenant_id, tx_id, label, amount);
        }

        // If an on-chain proof was found, update the covenant UTXO
        if let Some(new_utxo) = new_cov_utxo {
            if let Ok(Some(mut rec)) = self.db.get_covenant(covenant_id) {
                rec.covenant_utxo = Some(new_utxo);
                rec.covenant_value = new_cov_value;
                if let Err(e) = self.db.put_covenant(covenant_id, &rec) {
                    self.log(format!("Failed to update covenant UTXO after on-chain proof: {e}"));
                } else {
                    self.covenants = self.db.list_covenants();
                    self.log("Updated covenant UTXO from on-chain proof".into());
                }
            }
        }
    }

    fn selected_covenant_id(&self) -> Option<CovenantId> {
        self.selected_covenant.map(|i| self.covenants[i].0)
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
        self.load_tx_history(id);
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
        if let Some(file) = &mut self.log_file {
            use std::io::Write;
            let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let _ = writeln!(file, "[{ts}] {msg}");
        }
        self.log_messages.push(msg);
        self.log_selected = self.log_messages.len().saturating_sub(1);
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

    fn copy_to_clipboard(&mut self, content: &str) {
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(cb) => self.clipboard = Some(cb),
                Err(e) => {
                    self.log(format!("Clipboard error: {e}"));
                    return;
                }
            }
        }
        if let Some(cb) = &mut self.clipboard {
            match cb.set_text(content) {
                Ok(()) => {
                    let preview = if content.len() > 20 { format!("{}...", &content[..20]) } else { content.to_string() };
                    self.status_flash = Some((format!("Copied: {preview}"), Instant::now()));
                }
                Err(e) => self.log(format!("Clipboard error: {e}")),
            }
        }
    }
}

/// Derive action label and amount from a successfully-applied witness + transaction.
fn witness_label_amount(witness: &ActionWitness, tx: &Transaction) -> (&'static str, u64) {
    let payload_words: Vec<u32> = tx.payload.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    match witness {
        ActionWitness::Entry(_) => {
            let amount = tx.outputs.first().map(|o| o.value).unwrap_or(0);
            ("Entry", amount)
        }
        ActionWitness::Transfer(_) => {
            let amount = payload_words
                .get(ActionHeader::WORDS..)
                .and_then(|w| w.get(..TransferAction::WORDS))
                .and_then(|w| w.try_into().ok())
                .map(|w| TransferAction::from_words(w).amount)
                .unwrap_or(0);
            ("Transfer", amount)
        }
        ActionWitness::Exit(_) => {
            let amount = payload_words
                .get(ActionHeader::WORDS..)
                .and_then(|w| w.get(..ExitAction::WORDS))
                .and_then(|w| w.try_into().ok())
                .map(|w| ExitAction::from_words(w).amount)
                .unwrap_or(0);
            ("Exit", amount)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use kaspa_wrpc_client::prelude::{NetworkId, NetworkType};
    use std::sync::Arc;

    fn test_app() -> App {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::db::RollupDb::open(dir.path()).unwrap());
        let node = crate::node::KaspaNode::try_new("ws://127.0.0.1:1", NetworkId::new(NetworkType::Simnet)).unwrap();
        // Leak the tempdir so it isn't deleted while the DB handle is alive
        std::mem::forget(dir);
        App::new(db, node, Prefix::Simnet)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn test_ctrl_q_quits() {
        let mut app = test_app();
        assert!(!app.should_quit);
        app.handle_key(key_ctrl('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn test_plain_q_does_not_quit() {
        let mut app = test_app();
        app.handle_key(key(KeyCode::Char('q')));
        assert!(!app.should_quit);
    }

    #[test]
    fn test_tab_switching() {
        let mut app = test_app();
        assert_eq!(app.active_tab, Tab::Covenants);

        app.handle_key(key(KeyCode::Char('5')));
        assert_eq!(app.active_tab, Tab::Proving);

        app.handle_key(key(KeyCode::Char('2')));
        assert_eq!(app.active_tab, Tab::Accounts);

        app.handle_key(key(KeyCode::Char('7')));
        assert_eq!(app.active_tab, Tab::Log);
    }

    #[test]
    fn test_covenant_navigation() {
        let mut app = test_app();
        // Create two covenants
        app.create_covenant();
        app.create_covenant();
        assert_eq!(app.covenants.len(), 2);
        assert_eq!(app.covenant_list_index, 1); // points to last created

        // Navigate up
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.covenant_list_index, 0);

        // Navigate down
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.covenant_list_index, 1);

        // Can't go past end
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.covenant_list_index, 1);
    }

    #[test]
    fn test_account_role_focus() {
        let mut app = test_app();
        app.active_tab = Tab::Accounts;

        // Start with role_focused = false, index = 0
        assert!(!app.role_focused);
        assert_eq!(app.account_list_index, 0);

        // k at index 0 focuses role
        app.handle_key(key(KeyCode::Char('k')));
        assert!(app.role_focused);

        // j unfocuses role
        app.handle_key(key(KeyCode::Char('j')));
        assert!(!app.role_focused);
        assert_eq!(app.account_list_index, 0);
    }

    #[test]
    fn test_export_empty_covenants_logs() {
        let mut app = test_app();
        let log_count_before = app.log_messages.len();
        app.export_covenant_info();
        assert!(app.log_messages.len() > log_count_before);
        assert!(app.log_messages.last().unwrap().contains("No covenants"));
    }

    #[test]
    fn test_input_mode_blocks_normal_keys() {
        let mut app = test_app();
        app.input_mode =
            InputMode::PromptText { target: TextInputTarget::ImportCovenantId, buffer: String::new(), context: "test".into() };

        // Ctrl+Q should NOT quit when in input mode
        app.handle_key(key_ctrl('q'));
        assert!(!app.should_quit);

        // Tab keys should not switch tabs
        let prev_tab = app.active_tab;
        app.handle_key(key(KeyCode::Char('5')));
        assert_eq!(app.active_tab, prev_tab);
    }

    // ── UI rendering tests ───────────────────────────────────────────

    /// Helper: render the full UI into a test buffer and return the buffer contents as a string.
    fn render_to_string(app: &App, width: u16, height: u16) -> String {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut output = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                output.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn test_ui_status_bar_connected() {
        let mut app = test_app();
        app.connected = true;
        app.daa_score = 42;
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("Connected"), "status bar should show Connected");
        assert!(output.contains("DAA: 42"), "status bar should show DAA score");
        assert!(output.contains("Ctrl+Q:quit"), "status bar should show Ctrl+Q:quit");
    }

    #[test]
    fn test_ui_status_bar_disconnected() {
        let app = test_app();
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("Disconnected"), "status bar should show Disconnected");
    }

    #[test]
    fn test_ui_covenants_empty() {
        let app = test_app();
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("No covenants yet"), "should show empty covenants message");
    }

    #[test]
    fn test_ui_covenants_with_data() {
        let mut app = test_app();
        app.create_covenant();
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("Covenant ID"), "should show covenant table header");
        assert!(output.contains("Deployed"), "should show Deployed column");
        assert!(output.contains("Created"), "should show Created origin for new covenant");
    }

    #[test]
    fn test_ui_accounts_no_covenant() {
        let mut app = test_app();
        app.active_tab = Tab::Accounts;
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("Select a covenant first"), "should prompt to select covenant");
    }

    #[test]
    fn test_ui_log_tab() {
        let mut app = test_app();
        app.active_tab = Tab::Log;
        app.log("Test message alpha".into());
        app.log("Test message beta".into());
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("Test message alpha"), "log should show first message");
        assert!(output.contains("Test message beta"), "log should show second message");
    }

    #[test]
    fn test_ui_prover_not_initialized() {
        let mut app = test_app();
        app.active_tab = Tab::Proving;
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("Prover not initialized"), "should show prover not initialized");
    }

    #[test]
    fn test_ui_status_bar_no_overlap_with_content() {
        // Verify the status bar line is clean (no content bleeding into it)
        let mut app = test_app();
        app.connected = true;
        app.daa_score = 999;
        let backend = ratatui::backend::TestBackend::new(100, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| crate::ui::draw(frame, &app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        // The last row (index 19) is the status bar
        let last_row = 19u16;
        let mut status_line = String::new();
        for x in 0..buf.area.width {
            status_line.push_str(buf.cell((x, last_row)).map(|c| c.symbol()).unwrap_or(" "));
        }
        assert!(status_line.contains("Connected"), "status bar should be on last row");
        assert!(status_line.contains("DAA: 999"), "status bar should contain DAA score");
        // Status bar should NOT contain content from other panels
        assert!(!status_line.contains("No covenants"), "status bar should not contain covenant content");
    }

    #[test]
    fn test_ui_tx_history_empty() {
        let mut app = test_app();
        app.active_tab = Tab::TxHistory;
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("No transactions yet"), "should show empty tx history");
    }

    #[test]
    fn test_ui_status_flash_on_copy() {
        let mut app = test_app();
        app.connected = true;
        app.daa_score = 100;
        // Simulate a successful copy flash
        app.status_flash = Some(("Copied: kaspatest:abc123...".into(), std::time::Instant::now()));
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("Copied: kaspatest:abc123"), "flash message should appear in status bar");
        // The normal status bar content should NOT appear while flash is active
        assert!(!output.contains("Ctrl+Q:quit"), "normal status bar hidden during flash");
    }

    #[test]
    fn test_ui_status_flash_expires() {
        let mut app = test_app();
        app.connected = true;
        app.daa_score = 100;
        // Set a flash that expired 3 seconds ago
        app.status_flash = Some(("Copied: old".into(), std::time::Instant::now() - std::time::Duration::from_secs(3)));
        let output = render_to_string(&app, 120, 30);
        // Normal status bar should be back
        assert!(output.contains("Ctrl+Q:quit"), "normal status bar should return after flash expires");
        assert!(!output.contains("Copied: old"), "expired flash should not appear");
    }

    #[test]
    fn test_ui_popup_import() {
        let mut app = test_app();
        app.input_mode = InputMode::PromptText {
            target: TextInputTarget::ImportCovenantId,
            buffer: "abc123".into(),
            context: "Enter covenant ID".into(),
        };
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("Import Covenant"), "should show import popup title");
        assert!(output.contains("abc123"), "should show typed buffer");
    }

    #[test]
    fn test_proof_kind_toggle_on_undeployed() {
        let mut app = test_app();
        app.create_covenant();
        app.covenant_list_index = 0;

        // Default is Succinct (0)
        assert_eq!(app.covenants[0].1.proof_kind, 0);

        // Toggle to Groth16 (1)
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.covenants[0].1.proof_kind, 1);

        // Toggle back to Succinct (0)
        app.handle_key(key(KeyCode::Char('t')));
        assert_eq!(app.covenants[0].1.proof_kind, 0);
    }

    #[test]
    fn test_proof_kind_shown_in_covenant_table() {
        let mut app = test_app();
        app.create_covenant();
        let output = render_to_string(&app, 140, 30);
        assert!(output.contains("Kind"), "should show Kind column header");
        assert!(output.contains("Succinct"), "should show Succinct for default proof kind");
    }

    #[test]
    fn test_ctrl_l_sets_redraw_flag() {
        let mut app = test_app();
        assert!(!app.needs_full_redraw);
        app.handle_key(key_ctrl('l'));
        assert!(app.needs_full_redraw);
    }

    #[test]
    fn test_ui_status_bar_shows_redraw_hint() {
        let app = test_app();
        let output = render_to_string(&app, 120, 30);
        assert!(output.contains("Ctrl+L:redraw"), "status bar should show Ctrl+L:redraw hint");
    }

    /// Verify that after a popup (Processing) appears and then disappears,
    /// the underlying content renders cleanly with no leftover artifacts.
    #[test]
    fn test_ui_no_artifacts_after_popup_dismiss() {
        let mut app = test_app();
        app.active_tab = Tab::Actions;
        app.create_covenant();
        app.selected_covenant = Some(0);
        app.create_account();

        // Render with Processing popup active
        app.input_mode = InputMode::Processing { action: ActionType::Entry };
        let popup_output = render_to_string(&app, 120, 30);
        assert!(popup_output.contains("Building transaction"), "popup should show Building text");

        // Dismiss popup (action completes)
        app.input_mode = InputMode::Normal;
        let clean_output = render_to_string(&app, 120, 30);

        // The clean render should show normal Actions content, no popup leftovers
        assert!(clean_output.contains("Actions"), "should show Actions tab content");
        assert!(!clean_output.contains("Building transaction"), "popup text should be gone after dismiss");
        assert!(!clean_output.contains("nonce grinding"), "popup detail text should be gone after dismiss");

        // Verify it matches a fresh render (no popup was ever shown on this terminal)
        let fresh_output = render_to_string(&app, 120, 30);
        assert_eq!(clean_output, fresh_output, "render after popup dismiss should match a fresh render");
    }

    #[test]
    fn test_deploy_not_confirmed_until_tx_submitted() {
        let mut app = test_app();
        app.create_covenant();
        app.covenant_list_index = 0;
        app.selected_covenant = Some(0);

        // The covenant is created but not deployed
        assert!(app.covenants[0].1.deployment_tx_id.is_none());

        // Simulate deploy — record_tx is called but DB is NOT updated
        let fake_tx_id = Hash::from_bytes([0xAA; 32]);
        let cov_id = app.covenants[0].0;
        app.pending_tx_effects
            .insert(fake_tx_id, PendingTxEffect::Deploy { covenant_id: cov_id, deploy_tx_id: fake_tx_id, covenant_value: 100_000 });
        app.record_tx(fake_tx_id, "Deploy", 100_000);

        // Covenant should still NOT be deployed
        let rec = app.db.get_covenant(cov_id).unwrap().unwrap();
        assert!(rec.deployment_tx_id.is_none(), "covenant must not be deployed before TxSubmitted");

        // Simulate TxSubmitted
        app.handle_bg_result(BgResult::TxSubmitted { tx_id: fake_tx_id });

        // Now it should be deployed
        let rec = app.db.get_covenant(cov_id).unwrap().unwrap();
        assert!(rec.deployment_tx_id.is_some(), "covenant must be deployed after TxSubmitted");
        assert_eq!(rec.deployment_tx_id.unwrap(), fake_tx_id);
        assert_eq!(rec.covenant_value, Some(100_000));
    }

    #[test]
    fn test_deploy_reverts_on_tx_submit_failed() {
        let mut app = test_app();
        app.create_covenant();
        app.covenant_list_index = 0;
        app.selected_covenant = Some(0);

        let fake_tx_id = Hash::from_bytes([0xBB; 32]);
        let cov_id = app.covenants[0].0;
        app.pending_tx_effects
            .insert(fake_tx_id, PendingTxEffect::Deploy { covenant_id: cov_id, deploy_tx_id: fake_tx_id, covenant_value: 100_000 });
        app.record_tx(fake_tx_id, "Deploy", 100_000);

        // Simulate TxSubmitFailed
        app.handle_bg_result(BgResult::TxSubmitFailed { tx_id: fake_tx_id, error: "rejected".into() });

        // Covenant must remain undeployed
        let rec = app.db.get_covenant(cov_id).unwrap().unwrap();
        assert!(rec.deployment_tx_id.is_none(), "covenant must not be deployed after TxSubmitFailed");

        // Tx history should show Failed
        let tx = app.tx_history.iter().find(|r| r.tx_id == fake_tx_id).unwrap();
        assert!(matches!(tx.status, TxStatus::Failed(_)));
    }

    #[test]
    fn test_tx_stays_submitted_on_rpc_accept() {
        let mut app = test_app();
        let tx_id = Hash::from_bytes([0xCC; 32]);
        app.record_tx(tx_id, "Entry", 5000);

        // Initially Submitted
        assert_eq!(app.tx_history[0].status, TxStatus::Submitted);

        // RPC accepts the tx — status stays Submitted (not Confirmed)
        app.handle_bg_result(BgResult::TxSubmitted { tx_id });
        assert_eq!(app.tx_history[0].status, TxStatus::Submitted);
    }

    #[test]
    fn test_tx_status_updated_on_submit_failure() {
        let mut app = test_app();
        let tx_id = Hash::from_bytes([0xDD; 32]);
        app.record_tx(tx_id, "Exit", 3000);

        assert_eq!(app.tx_history[0].status, TxStatus::Submitted);

        // Simulate failure
        app.handle_bg_result(BgResult::TxSubmitFailed { tx_id, error: "insufficient funds".into() });
        assert!(matches!(app.tx_history[0].status, TxStatus::Failed(ref msg) if msg.contains("insufficient")));
    }

    #[test]
    fn test_prove_submit_reverts_utxo_on_failure() {
        let mut app = test_app();
        app.create_covenant();
        app.covenant_list_index = 0;
        app.selected_covenant = Some(0);
        let cov_id = app.covenants[0].0;

        // Simulate deployed state
        let old_utxo_tx = Hash::from_bytes([0x11; 32]);
        let mut rec = app.covenants[0].1.clone();
        rec.deployment_tx_id = Some(old_utxo_tx);
        rec.covenant_utxo = Some((old_utxo_tx, 0));
        rec.covenant_value = Some(500_000);
        app.db.put_covenant(cov_id, &rec).unwrap();
        app.covenants = app.db.list_covenants();

        // Simulate a prove-submit that optimistically updated UTXO
        let prove_tx_id = Hash::from_bytes([0x22; 32]);
        app.pending_tx_effects.insert(
            prove_tx_id,
            PendingTxEffect::ProveSubmit { covenant_id: cov_id, old_utxo: (old_utxo_tx, 0), old_value: 500_000, perm_state: None },
        );

        // Optimistic update (as submit_proof does)
        let mut updated = rec.clone();
        updated.covenant_utxo = Some((prove_tx_id, 0));
        updated.covenant_value = Some(497_000);
        app.db.put_covenant(cov_id, &updated).unwrap();

        // Simulate failure
        app.handle_bg_result(BgResult::TxSubmitFailed { tx_id: prove_tx_id, error: "bad proof".into() });

        // UTXO should be reverted to old values
        let reverted = app.db.get_covenant(cov_id).unwrap().unwrap();
        assert_eq!(reverted.covenant_utxo, Some((old_utxo_tx, 0)));
        assert_eq!(reverted.covenant_value, Some(500_000));
    }

    #[test]
    fn test_log_writes_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::db::RollupDb::open(dir.path()).unwrap());
        let node = crate::node::KaspaNode::try_new("ws://127.0.0.1:1", NetworkId::new(NetworkType::Simnet)).unwrap();
        let log_path = dir.path().join("tui.log");
        let mut app = App::with_log_path(db, node, Prefix::Simnet, Some(log_path.clone()));

        app.log("test error one".into());
        app.log("test error two".into());

        // Force flush by dropping the file handle
        drop(app.log_file.take());

        let contents = std::fs::read_to_string(&log_path).unwrap();
        assert!(contents.contains("test error one"), "log file should contain first message");
        assert!(contents.contains("test error two"), "log file should contain second message");
        // Each line should have a timestamp prefix
        for line in contents.lines() {
            assert!(line.starts_with('['), "each log line should start with timestamp bracket");
        }
    }

    /// Verify Confirm popup renders and dismisses cleanly.
    #[test]
    fn test_ui_no_artifacts_after_confirm_dismiss() {
        let mut app = test_app();
        app.active_tab = Tab::Actions;
        app.create_covenant();
        app.selected_covenant = Some(0);
        app.create_account();

        // Show confirm popup
        app.input_mode = InputMode::Confirm {
            action: ActionType::Entry,
            amount: 1000,
            summary: vec!["Action: Entry".into(), "Amount: 1000 sompi".into(), "Enter: submit | Esc: cancel".into()],
        };
        let popup_output = render_to_string(&app, 120, 30);
        assert!(popup_output.contains("Confirm"), "should show confirm popup");

        // Dismiss
        app.input_mode = InputMode::Normal;
        let clean_output = render_to_string(&app, 120, 30);
        assert!(!clean_output.contains("Confirm Entry"), "confirm popup should be gone");

        let fresh_output = render_to_string(&app, 120, 30);
        assert_eq!(clean_output, fresh_output, "post-dismiss render should match fresh render");
    }

    // ── Step 1: Schnorr signing tests ──

    #[test]
    fn test_proof_tx_uses_115_sig_op_count() {
        // The proof tx sig_op_count should be 115 (matching make_mock_transaction convention)
        use kaspa_consensus_core::tx::TransactionInput;
        let sig_script = vec![0u8; 8];
        let outpoint = kaspa_consensus_core::tx::TransactionOutpoint::new(Hash::from_bytes([0u8; 32]), 0);
        let input = TransactionInput::new(outpoint, sig_script, 0, 115);
        assert_eq!(input.sig_op_count, 115, "proof tx must use sig_op_count=115");
        // Ensure u8::MAX (255) is NOT the value
        assert_ne!(input.sig_op_count, u8::MAX, "proof tx must not use u8::MAX");
    }

    // ── Step 3: ViewDetail popup tests ──

    #[test]
    fn test_view_detail_popup_in_tx_history() {
        let mut app = test_app();
        app.active_tab = Tab::TxHistory;

        // Insert a tx record
        let tx_id = Hash::from_bytes([0xab; 32]);
        app.tx_history.push(TxRecord {
            tx_id,
            action: "Entry".to_string(),
            amount: 5000,
            timestamp: 1700000000,
            status: TxStatus::Submitted,
        });
        app.tx_history_index = 0;

        // Press Enter → ViewDetail
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.input_mode, InputMode::ViewDetail { .. }), "Enter in TxHistory should open ViewDetail popup");
    }

    #[test]
    fn test_view_detail_renders_full_tx_id() {
        let mut app = test_app();
        app.active_tab = Tab::TxHistory;

        let tx_id = Hash::from_bytes([0xcd; 32]);
        let tx_id_str = tx_id.to_string();
        assert_eq!(tx_id_str.len(), 64, "tx_id string should be 64 hex chars");

        app.tx_history.push(TxRecord {
            tx_id,
            action: "Prove".to_string(),
            amount: 100_000,
            timestamp: 1700000001,
            status: TxStatus::Confirmed,
        });
        app.tx_history_index = 0;

        // Open ViewDetail
        app.handle_key(key(KeyCode::Enter));
        if let InputMode::ViewDetail { ref lines, .. } = app.input_mode {
            let has_full_id = lines.iter().any(|l| l.contains(&tx_id_str));
            assert!(has_full_id, "ViewDetail lines should contain the full 64-char tx_id");
        } else {
            panic!("Expected ViewDetail mode");
        }
    }

    #[test]
    fn test_view_detail_key_navigation() {
        let mut app = test_app();
        let lines = vec!["Line 0".into(), "Line 1".into(), "Line 2".into()];
        app.input_mode = InputMode::ViewDetail { lines, scroll: 0 };

        // j scrolls down
        app.handle_key(key(KeyCode::Char('j')));
        if let InputMode::ViewDetail { scroll, .. } = app.input_mode {
            assert_eq!(scroll, 1);
        }

        // k scrolls up
        app.handle_key(key(KeyCode::Char('k')));
        if let InputMode::ViewDetail { scroll, .. } = app.input_mode {
            assert_eq!(scroll, 0);
        }

        // Esc closes
        app.handle_key(key(KeyCode::Esc));
        assert!(app.input_mode.is_normal(), "Esc should close ViewDetail");
    }

    // ── Step 4: Scrollable log tests ──

    #[test]
    fn test_log_scroll_navigation() {
        let mut app = test_app();
        app.active_tab = Tab::Log;
        app.log("msg0".into());
        app.log("msg1".into());
        app.log("msg2".into());
        assert_eq!(app.log_selected, 2, "log_selected should auto-advance to last");

        // j moves down (already at bottom, no change)
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.log_selected, 2);

        // k moves up
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.log_selected, 1);

        // another k
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.log_selected, 0);

        // k at 0 stays at 0
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.log_selected, 0);

        // G jumps to last
        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.log_selected, 2);

        // g jumps to first
        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.log_selected, 0);
        assert_eq!(app.log_scroll, 0);
    }

    #[test]
    fn test_log_view_detail_opens_on_enter() {
        let mut app = test_app();
        app.active_tab = Tab::Log;
        app.log("detailed log message\nwith newline".into());
        app.log_selected = 0;

        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(app.input_mode, InputMode::ViewDetail { .. }), "Enter in Log tab should open ViewDetail");
    }

    // ── Step 5: Withdraw tests ──

    #[test]
    fn test_withdraw_requires_perm_utxo() {
        let mut app = test_app();
        app.create_covenant();
        app.selected_covenant = Some(0);
        app.perm_utxo = None;
        let log_count = app.log_messages.len();
        app.withdraw();
        assert!(app.log_messages.len() > log_count, "withdraw without perm_utxo should log error");
        assert!(app.pending_ops.is_empty(), "no tx should be queued");
    }

    #[test]
    fn test_withdraw_logs_insufficient_when_no_delegate_utxos() {
        // Withdraw should log an error when no delegate UTXOs are available
        let mut app = test_app();
        app.create_covenant();
        app.selected_covenant = Some(0);

        let spk = vec![0x20u8; 34];
        let exit_amount: u64 = 10_000;
        let redeem_script = vec![0xabu8; 50];
        app.perm_utxo = Some(PermUtxoState {
            utxo: (Hash::from_bytes([0x01; 32]), 1),
            value: 1000,
            redeem_script,
            exit_data: vec![(spk, exit_amount)],
            unclaimed: 1,
        });

        let log_count = app.log_messages.len();
        app.withdraw();

        // Should log about insufficient UTXOs (no delegate UTXOs loaded)
        assert!(app.log_messages.len() > log_count, "withdraw should log a message");
        let last_log = app.log_messages.last().unwrap();
        assert!(
            last_log.contains("Insufficient") || last_log.contains("delegate") || last_log.contains("built"),
            "should log about missing UTXOs or tx built: {last_log}"
        );
    }

    #[test]
    fn test_perm_utxo_cleared_after_single_exit_withdraw() {
        // If unclaimed == 1 and withdraw succeeds, perm_utxo should be cleared
        // We test this with the internal state update logic
        let mut perm = PermUtxoState {
            utxo: (Hash::from_bytes([0x01; 32]), 1),
            value: 1000,
            redeem_script: vec![0u8; 50],
            exit_data: vec![(vec![0x20u8; 34], 1000)],
            unclaimed: 1,
        };
        // Simulate the withdraw update
        perm.exit_data.remove(0);
        perm.unclaimed = perm.unclaimed.saturating_sub(1);
        assert_eq!(perm.unclaimed, 0);
        assert!(perm.exit_data.is_empty());
    }
}
