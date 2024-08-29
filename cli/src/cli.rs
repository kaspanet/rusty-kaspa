use crate::error::Error;
use crate::helpers::*;
use crate::imports::*;
use crate::modules::miner::Miner;
use crate::modules::node::Node;
use crate::notifier::{Notification, Notifier};
use crate::result::Result;
use kaspa_daemon::{DaemonEvent, DaemonKind, Daemons};
use kaspa_wallet_core::account::Account;
use kaspa_wallet_core::rpc::DynRpcApi;
use kaspa_wallet_core::storage::{IdT, PrvKeyDataInfo};
use kaspa_wrpc_client::{KaspaRpcClient, Resolver};
use workflow_core::channel::*;
use workflow_core::time::Instant;
use workflow_log::*;
pub use workflow_terminal::Event as TerminalEvent;
use workflow_terminal::*;
pub use workflow_terminal::{Options as TerminalOptions, TargetElement as TerminalTarget};

const NOTIFY: &str = "\x1B[2m⎟\x1B[0m";

pub struct Options {
    pub daemons: Option<Arc<Daemons>>,
    pub terminal: TerminalOptions,
}

impl Options {
    pub fn new(terminal_options: TerminalOptions, daemons: Option<Arc<Daemons>>) -> Self {
        Self { daemons, terminal: terminal_options }
    }
}

pub struct KaspaCli {
    term: Arc<Mutex<Option<Arc<Terminal>>>>,
    wallet: Arc<Wallet>,
    notifications_task_ctl: DuplexChannel,
    mute: Arc<AtomicBool>,
    flags: Flags,
    last_interaction: Arc<Mutex<Instant>>,
    daemons: Arc<Daemons>,
    handlers: Arc<HandlerCli>,
    shutdown: Arc<AtomicBool>,
    node: Mutex<Option<Arc<Node>>>,
    miner: Mutex<Option<Arc<Miner>>>,
    notifier: Notifier,
    sync_state: Mutex<Option<SyncState>>,
}

impl From<&KaspaCli> for Arc<Terminal> {
    fn from(ctx: &KaspaCli) -> Arc<Terminal> {
        ctx.term()
    }
}

impl AsRef<KaspaCli> for KaspaCli {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl workflow_log::Sink for KaspaCli {
    fn write(&self, _target: Option<&str>, _level: Level, args: &std::fmt::Arguments<'_>) -> bool {
        if let Some(term) = self.try_term() {
            cfg_if! {
                if #[cfg(target_arch = "wasm32")] {
                    if _level == Level::Error {
                        term.writeln(style(args.to_string().crlf()).red().to_string());
                    }
                    false
                } else {
                    match _level {
                        Level::Error => {
                            term.writeln(style(args.to_string().crlf()).red().to_string());
                        },
                        _ => {
                            term.writeln(args.to_string());
                        }
                    }
                    true
                }
            }
        } else {
            false
        }
    }
}

impl KaspaCli {
    pub fn init() {
        cfg_if! {
            if #[cfg(not(target_arch = "wasm32"))] {
                init_panic_hook(||{
                    std::println!("halt");
                    1
                });
                kaspa_core::log::init_logger(None, "info");
            } else {
                kaspa_core::log::set_log_level(LevelFilter::Info);
            }
        }

        workflow_log::set_colors_enabled(true);
    }

    pub async fn try_new_arc(options: Options) -> Result<Arc<Self>> {
        let wallet = Arc::new(Wallet::try_new(Wallet::local_store()?, Some(Resolver::default()), None)?);

        let kaspa_cli = Arc::new(KaspaCli {
            term: Arc::new(Mutex::new(None)),
            wallet,
            notifications_task_ctl: DuplexChannel::oneshot(),
            mute: Arc::new(AtomicBool::new(true)),
            flags: Flags::default(),
            last_interaction: Arc::new(Mutex::new(Instant::now())),
            handlers: Arc::new(HandlerCli::default()),
            daemons: options.daemons.unwrap_or_default(),
            shutdown: Arc::new(AtomicBool::new(false)),
            node: Mutex::new(None),
            miner: Mutex::new(None),
            notifier: Notifier::try_new()?,
            sync_state: Mutex::new(None),
        });

        let term = Arc::new(Terminal::try_new_with_options(kaspa_cli.clone(), options.terminal)?);
        term.init().await?;

        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                kaspa_cli.init_panic_hook();
            }
        }

        Ok(kaspa_cli)
    }

    pub fn term(&self) -> Arc<Terminal> {
        self.term.lock().unwrap().as_ref().cloned().expect("WalletCli::term is not initialized")
    }

    pub fn try_term(&self) -> Option<Arc<Terminal>> {
        self.term.lock().unwrap().as_ref().cloned()
    }

    pub fn notifier(&self) -> &Notifier {
        &self.notifier
    }

    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    pub fn wallet(&self) -> Arc<Wallet> {
        self.wallet.clone()
    }

    pub fn is_connected(&self) -> bool {
        self.wallet.is_connected()
    }

    pub fn rpc_api(&self) -> Arc<DynRpcApi> {
        self.wallet.rpc_api().clone()
    }

    pub fn try_rpc_api(&self) -> Option<Arc<DynRpcApi>> {
        self.wallet.try_rpc_api().clone()
    }

    pub fn try_rpc_client(&self) -> Option<Arc<KaspaRpcClient>> {
        self.wallet.try_wrpc_client().clone()
    }

    pub fn store(&self) -> Arc<dyn Interface> {
        self.wallet.store().clone()
    }

    pub fn daemons(&self) -> &Arc<Daemons> {
        &self.daemons
    }

    pub fn handlers(&self) -> Arc<HandlerCli> {
        self.handlers.clone()
    }

    pub fn flags(&self) -> &Flags {
        &self.flags
    }

    pub fn toggle_mute(&self) -> &'static str {
        helpers::toggle(&self.mute)
    }

    pub fn is_mutted(&self) -> bool {
        self.mute.load(Ordering::SeqCst)
    }

    pub fn register_metrics(self: &Arc<Self>) -> Result<()> {
        use crate::modules::metrics;
        register_handlers!(self, self.handlers(), [metrics]);
        Ok(())
    }

    pub fn register_handlers(self: &Arc<Self>) -> Result<()> {
        crate::modules::register_handlers(self)?;

        if let Some(node) = self.handlers().get("node") {
            let node = node.downcast_arc::<crate::modules::node::Node>().ok();
            *self.node.lock().unwrap() = node;
        }

        if let Some(miner) = self.handlers().get("miner") {
            let miner = miner.downcast_arc::<crate::modules::miner::Miner>().ok();
            *self.miner.lock().unwrap() = miner;
        }

        crate::matchers::register_link_matchers(self)?;

        Ok(())
    }

    pub async fn handle_daemon_event(self: &Arc<Self>, event: DaemonEvent) -> Result<()> {
        match event.kind() {
            DaemonKind::Kaspad => {
                let node = self.node.lock().unwrap().clone();
                if let Some(node) = node {
                    node.handle_event(self, event.into()).await?;
                } else {
                    panic!("Stdio handler: node module is not initialized");
                }
            }
            DaemonKind::CpuMiner => {
                let miner = self.miner.lock().unwrap().clone();
                if let Some(miner) = miner {
                    miner.handle_event(self, event.into()).await?;
                } else {
                    panic!("Stdio handler: miner module is not initialized");
                }
            }
        }

        Ok(())
    }

    pub async fn start(self: &Arc<Self>) -> Result<()> {
        self.start_notification_pipe_task();
        self.handlers.start(self).await?;
        // wallet starts rpc and notifier
        self.wallet.load_settings().await.unwrap_or_else(|_| log_error!("Unable to load settings, discarding..."));
        self.wallet.start().await?;
        Ok(())
    }

    pub async fn run(self: &Arc<Self>) -> Result<()> {
        self.term().run().await?;
        Ok(())
    }

    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.wallet.stop().await?;

        self.handlers.stop(self).await?;

        // stop notification pipe task
        self.stop_notification_pipe_task().await?;
        Ok(())
    }

    async fn stop_notification_pipe_task(self: &Arc<Self>) -> Result<()> {
        self.notifications_task_ctl.signal(()).await?;
        Ok(())
    }

    fn start_notification_pipe_task(self: &Arc<Self>) {
        let this = self.clone();
        let multiplexer = MultiplexerChannel::from(self.wallet.multiplexer());

        workflow_core::task::spawn(async move {
            loop {
                select! {

                    _ = this.notifications_task_ctl.request.receiver.recv().fuse() => {
                        break;
                    },

                    msg = multiplexer.receiver.recv().fuse() => {

                        if let Ok(msg) = msg {
                            match *msg {
                                Events::WalletPing => {
                                    // log_info!("Kaspa NG - received wallet ping");
                                },
                                Events::Metrics { network_id : _, metrics : _ } => {
                                    // log_info!("Kaspa NG - received metrics event {metrics:?}")
                                }
                                Events::Error { message } => { terrorln!(this,"{message}"); },
                                Events::UtxoProcStart => {},
                                Events::UtxoProcStop => {},
                                Events::UtxoProcError { message } => {
                                    terrorln!(this,"{message}");
                                },
                                #[allow(unused_variables)]
                                Events::Connect{ url, network_id } => {
                                    // log_info!("Connected to {url}");
                                },
                                #[allow(unused_variables)]
                                Events::Disconnect{ url, network_id } => {
                                    tprintln!(this, "Disconnected from {}",url.unwrap_or("N/A".to_string()));
                                    this.term().refresh_prompt();
                                },
                                Events::UtxoIndexNotEnabled { .. } => {
                                    tprintln!(this, "Error: Kaspa node UTXO index is not enabled...")
                                },
                                Events::SyncState { sync_state } => {

                                    if sync_state.is_synced() && this.wallet().is_open() {
                                        let guard = this.wallet().guard();
                                        let guard = guard.lock().await;
                                        if let Err(error) = this.wallet().reload(false, &guard).await {
                                            terrorln!(this, "Unable to reload wallet: {error}");
                                        }
                                    }

                                    this.sync_state.lock().unwrap().replace(sync_state);
                                    this.term().refresh_prompt();
                                }
                                Events::ServerStatus {
                                    is_synced,
                                    server_version,
                                    url,
                                    ..
                                } => {

                                    tprintln!(this, "Connected to Kaspa node version {server_version} at {}", url.unwrap_or("N/A".to_string()));

                                    let is_open = this.wallet.is_open();

                                    if !is_synced {
                                        if is_open {
                                            terrorln!(this, "Unable to update the wallet state - Kaspa node is currently syncing with the network...");

                                        } else {
                                            terrorln!(this, "Kaspa node is currently syncing with the network, please wait for the sync to complete...");
                                        }
                                    }

                                    this.term().refresh_prompt();

                                },
                                Events::WalletHint {
                                    hint
                                } => {

                                    if let Some(hint) = hint {
                                        tprintln!(this, "\nYour wallet hint is: {hint}\n");
                                    }

                                },
                                Events::AccountSelection { .. } => { },
                                Events::WalletCreate { .. } => { },
                                Events::WalletError { .. } => { },
                                // Events::WalletReady { .. } => { },

                                Events::WalletOpen { .. } |
                                Events::WalletReload { .. } => { },
                                Events::WalletClose => {
                                    this.term().refresh_prompt();
                                },
                                Events::PrvKeyDataCreate { .. } => { },
                                Events::AccountDeactivation { .. } => { },
                                Events::AccountActivation { .. } => {
                                    // list all accounts
                                    this.list().await.unwrap_or_else(|err|terrorln!(this, "{err}"));

                                    // load default account if only one account exists
                                    this.wallet().autoselect_default_account_if_single().await.ok();
                                    this.term().refresh_prompt();
                                },
                                Events::AccountCreate { .. } => { },
                                Events::AccountUpdate { .. } => { },
                                Events::DaaScoreChange { current_daa_score } => {
                                    if this.is_mutted() && this.flags.get(Track::Daa) {
                                        tprintln!(this, "{NOTIFY} DAA: {current_daa_score}");
                                    }
                                },
                                Events::Discovery { .. } => { }
                                Events::Reorg {
                                    record
                                } => {
                                    if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Pending)) {
                                        let guard = this.wallet.guard();
                                        let guard = guard.lock().await;

                                        let include_utxos = this.flags.get(Track::Utxo);
                                        let tx = record.format_transaction_with_state(&this.wallet,Some("reorg"),include_utxos, &guard).await;
                                        tx.iter().for_each(|line|tprintln!(this,"{NOTIFY} {line}"));
                                    }
                                },
                                Events::Stasis {
                                    record
                                } => {
                                    // Pending and coinbase stasis fall under the same `Track` category
                                    if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Pending)) {
                                        let guard = this.wallet.guard();
                                        let guard = guard.lock().await;

                                        let include_utxos = this.flags.get(Track::Utxo);
                                        let tx = record.format_transaction_with_state(&this.wallet,Some("stasis"),include_utxos, &guard).await;
                                        tx.iter().for_each(|line|tprintln!(this,"{NOTIFY} {line}"));
                                    }
                                },
                                // Events::External {
                                //     record
                                // } => {
                                //     if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Tx)) {
                                //         let include_utxos = this.flags.get(Track::Utxo);
                                //         let tx = record.format_with_state(&this.wallet,Some("external"),include_utxos).await;
                                //         tx.iter().for_each(|line|tprintln!(this,"{NOTIFY} {line}"));
                                //     }
                                // },
                                Events::Pending {
                                    record
                                } => {
                                    if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Pending)) {
                                        let guard = this.wallet.guard();
                                        let guard = guard.lock().await;

                                        let include_utxos = this.flags.get(Track::Utxo);
                                        let tx = record.format_transaction_with_state(&this.wallet,Some("pending"),include_utxos, &guard).await;
                                        tx.iter().for_each(|line|tprintln!(this,"{NOTIFY} {line}"));
                                    }
                                },
                                Events::Maturity {
                                    record
                                } => {
                                    if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Tx)) {
                                        let guard = this.wallet.guard();
                                        let guard = guard.lock().await;

                                        let include_utxos = this.flags.get(Track::Utxo);
                                        let tx = record.format_transaction_with_state(&this.wallet,Some("confirmed"),include_utxos, &guard).await;
                                        tx.iter().for_each(|line|tprintln!(this,"{NOTIFY} {line}"));
                                    }
                                },
                                // Events::Outgoing {
                                //     record
                                // } => {
                                //     if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Tx)) {
                                //         let include_utxos = this.flags.get(Track::Utxo);
                                //         let tx = record.format_with_state(&this.wallet,Some("confirmed"),include_utxos).await;
                                //         tx.iter().for_each(|line|tprintln!(this,"{NOTIFY} {line}"));
                                //     }
                                // },
                                // Events::Change {
                                //     record
                                // } => {
                                //     if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Tx)) {
                                //         let include_utxos = this.flags.get(Track::Utxo);
                                //         let tx = record.format_with_state(&this.wallet,Some("change"),include_utxos).await;
                                //         tx.iter().for_each(|line|tprintln!(this,"{NOTIFY} {line}"));
                                //     }
                                // },
                                Events::Balance {
                                    balance,
                                    id,
                                } => {

                                    if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Balance)) {
                                        let network_id = this.wallet.network_id().expect("missing network type");
                                        let network_type = NetworkType::from(network_id);
                                        let balance_strings = BalanceStrings::from((balance.as_ref(),&network_type, None));
                                        let id = id.short();

                                        let mature_utxo_count = balance.as_ref().map(|balance|balance.mature_utxo_count.separated_string()).unwrap_or("N/A".to_string());
                                        let pending_utxo_count = balance.as_ref().map(|balance|balance.pending_utxo_count).unwrap_or(0);

                                        let pending_utxo_info = if pending_utxo_count > 0 {
                                            format!("({} pending)", pending_utxo_count)
                                        } else { "".to_string() };
                                        let utxo_info = style(format!("{mature_utxo_count} UTXOs {pending_utxo_info}")).dim();

                                        tprintln!(this, "{NOTIFY} {} {id}: {balance_strings}   {utxo_info}",style("balance".pad_to_width(8)).blue());
                                    }

                                    this.term().refresh_prompt();
                                }
                            }
                        }
                    }
                }
            }

            this.notifications_task_ctl
                .response
                .sender
                .send(())
                .await
                .unwrap_or_else(|err| log_error!("WalletCli::notification_pipe_task() unable to signal task shutdown: `{err}`"));
        });
    }

    // ---

    /// Asks uses for a wallet secret, checks the supplied account's private key info
    /// and if it requires a payment secret, asks for it as well.
    pub(crate) async fn ask_wallet_secret(&self, account: Option<&Arc<dyn Account>>) -> Result<(Secret, Option<Secret>)> {
        let wallet_secret = Secret::new(self.term().ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());

        let payment_secret = if let Some(account) = account {
            if self.wallet().is_account_key_encrypted(account).await?.is_some_and(|f| f) {
                Some(Secret::new(self.term().ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()))
            } else {
                None
            }
        } else {
            None
        };

        Ok((wallet_secret, payment_secret))
    }

    pub async fn account(&self) -> Result<Arc<dyn Account>> {
        if let Ok(account) = self.wallet.account() {
            Ok(account)
        } else {
            let account = self.select_account().await?;
            self.wallet.select(Some(&account)).await?;
            Ok(account)
        }
    }

    pub async fn find_accounts_by_name_or_id(&self, pat: &str) -> Result<Arc<dyn Account>> {
        let matches = self.wallet().find_accounts_by_name_or_id(pat).await?;
        if matches.is_empty() {
            Err(Error::AccountNotFound(pat.to_string()))
        } else if matches.len() > 1 {
            Err(Error::AmbiguousAccount(pat.to_string()))
        } else {
            Ok(matches[0].clone())
        }
    }

    pub async fn prompt_account(&self) -> Result<Arc<dyn Account>> {
        self.select_account_with_args(false).await
    }

    pub async fn select_account(&self) -> Result<Arc<dyn Account>> {
        self.select_account_with_args(true).await
    }

    async fn select_account_with_args(&self, autoselect: bool) -> Result<Arc<dyn Account>> {
        let guard = self.wallet.guard();
        let guard = guard.lock().await;

        let mut selection = None;

        let mut list_by_key = Vec::<(Arc<PrvKeyDataInfo>, Vec<(usize, Arc<dyn Account>)>)>::new();
        let mut flat_list = Vec::<Arc<dyn Account>>::new();

        let mut keys = self.wallet.keys().await?;
        while let Some(key) = keys.try_next().await? {
            let mut prv_key_accounts = Vec::new();
            let mut accounts = self.wallet.accounts(Some(key.id), &guard).await?;
            while let Some(account) = accounts.next().await {
                let account = account?;
                prv_key_accounts.push((flat_list.len(), account.clone()));
                flat_list.push(account.clone());
            }

            list_by_key.push((key.clone(), prv_key_accounts));
        }

        let mut watch_accounts = Vec::<(usize, Arc<dyn Account>)>::new();
        let mut unfiltered_accounts = self.wallet.accounts(None, &guard).await?;

        while let Some(account) = unfiltered_accounts.try_next().await? {
            if account.feature().is_some() {
                watch_accounts.push((flat_list.len(), account.clone()));
                flat_list.push(account.clone());
            }
        }

        if flat_list.is_empty() {
            return Err(Error::NoAccounts);
        } else if autoselect && flat_list.len() == 1 {
            return Ok(flat_list.pop().unwrap());
        }

        while selection.is_none() {
            tprintln!(self);

            list_by_key.iter().for_each(|(prv_key_data_info, accounts)| {
                tprintln!(self, "• {prv_key_data_info}");

                accounts.iter().for_each(|(seq, account)| {
                    let seq = style(seq.to_string()).cyan();
                    let ls_string = account.get_list_string().unwrap_or_else(|err| panic!("{err}"));
                    tprintln!(self, "    {seq}: {ls_string}");
                })
            });

            if !watch_accounts.is_empty() {
                tprintln!(self, "• watch-only");
            }

            watch_accounts.iter().for_each(|(seq, account)| {
                let seq = style(seq.to_string()).cyan();
                let ls_string = account.get_list_string().unwrap_or_else(|err| panic!("{err}"));
                tprintln!(self, "    {seq}: {ls_string}");
            });

            tprintln!(self);

            let range = if flat_list.len() > 1 { format!("[{}..{}] ", 0, flat_list.len() - 1) } else { "".to_string() };

            let text =
                self.term().ask(false, &format!("Please select account {}or <enter> to abort: ", range)).await?.trim().to_string();
            if text.is_empty() {
                return Err(Error::UserAbort);
            } else {
                match text.parse::<usize>() {
                    Ok(seq) if seq < flat_list.len() => selection = flat_list.get(seq).cloned(),
                    _ => {}
                };
            }
        }

        let account = selection.unwrap();
        let ident = style(account.name_with_id()).blue();
        tprintln!(self, "selecting account: {ident}");

        Ok(account)
    }

    pub async fn select_private_key(&self) -> Result<Arc<PrvKeyDataInfo>> {
        self.select_private_key_with_args(true).await
    }

    pub async fn select_private_key_with_args(&self, autoselect: bool) -> Result<Arc<PrvKeyDataInfo>> {
        let mut selection = None;

        // let mut list_by_key = Vec::<(Arc<PrvKeyDataInfo>, Vec<(usize, Arc<dyn Account>)>)>::new();
        let mut flat_list = Vec::<Arc<PrvKeyDataInfo>>::new();

        let mut keys = self.wallet.keys().await?;
        while let Some(key) = keys.try_next().await? {
            flat_list.push(key);
        }

        if flat_list.is_empty() {
            return Err(Error::NoKeys);
        } else if autoselect && flat_list.len() == 1 {
            return Ok(flat_list.pop().unwrap());
        }

        while selection.is_none() {
            tprintln!(self);

            flat_list.iter().enumerate().for_each(|(seq, prv_key_data_info)| {
                tprintln!(self, "    {seq}: {prv_key_data_info}");
            });

            tprintln!(self);

            let range = if flat_list.len() > 1 { format!("[{}..{}] ", 0, flat_list.len() - 1) } else { "".to_string() };

            let text =
                self.term().ask(false, &format!("Please select private key {}or <enter> to abort: ", range)).await?.trim().to_string();
            if text.is_empty() {
                return Err(Error::UserAbort);
            } else {
                match text.parse::<usize>() {
                    Ok(seq) if seq < flat_list.len() => selection = flat_list.get(seq).cloned(),
                    _ => {}
                };
            }
        }

        let prv_key_data_info = selection.unwrap();
        tprintln!(self, "\nselecting private key: {prv_key_data_info}\n");

        Ok(prv_key_data_info)
    }

    pub async fn list(&self) -> Result<()> {
        let guard = self.wallet.guard();
        let guard = guard.lock().await;

        let mut keys = self.wallet.keys().await?;

        tprintln!(self);
        while let Some(key) = keys.try_next().await? {
            tprintln!(self, "• {}", style(&key).dim());

            let mut accounts = self.wallet.accounts(Some(key.id), &guard).await?;
            while let Some(account) = accounts.try_next().await? {
                let receive_address = account.receive_address()?;
                tprintln!(self, "    • {}", account.get_list_string()?);
                tprintln!(self, "      {}", style(receive_address.to_string()).blue());
            }
        }

        let mut unfiltered_accounts = self.wallet.accounts(None, &guard).await?;
        let mut feature_header_printed = false;
        while let Some(account) = unfiltered_accounts.try_next().await? {
            if let Some(feature) = account.feature() {
                if !feature_header_printed {
                    tprintln!(self, "{}", style("• watch-only").dim());
                    feature_header_printed = true;
                }
                tprintln!(self, "  • {}", account.get_list_string().unwrap());
                tprintln!(self, "      • {}", style(feature).cyan());
            }
        }
        tprintln!(self);

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        if !self.shutdown.load(Ordering::SeqCst) {
            self.shutdown.store(true, Ordering::SeqCst);

            tprintln!(self, "{}", style("shutting down...").magenta());

            let miner = self.daemons().try_cpu_miner();
            let kaspad = self.daemons().try_kaspad();

            if let Some(miner) = miner.as_ref() {
                miner.mute(false).await?;
                miner.stop().await?;
            }

            if let Some(kaspad) = kaspad.as_ref() {
                kaspad.mute(false).await?;
                kaspad.stop().await?;
            }

            if let Some(miner) = miner.as_ref() {
                miner.join().await?;
            }

            if let Some(kaspad) = kaspad.as_ref() {
                kaspad.join().await?;
            }

            self.term().exit().await;
        }

        Ok(())
    }

    fn sync_state(&self) -> Option<String> {
        if let Some(state) = self.sync_state.lock().unwrap().as_ref() {
            match state {
                SyncState::Proof { level } => {
                    if *level == 0 {
                        Some([style("SYNC").red().to_string(), style("...").black().to_string()].join(" "))
                    } else {
                        Some([style("SYNC PROOF").red().to_string(), style(level.separated_string()).dim().to_string()].join(" "))
                    }
                }
                SyncState::Headers { headers, progress } => Some(
                    [
                        style("SYNC IBD HDRS").red().to_string(),
                        style(format!("{} ({}%)", headers.separated_string(), progress)).dim().to_string(),
                    ]
                    .join(" "),
                ),
                SyncState::Blocks { blocks, progress } => Some(
                    [
                        style("SYNC IBD BLOCKS").red().to_string(),
                        style(format!("{} ({}%)", blocks.separated_string(), progress)).dim().to_string(),
                    ]
                    .join(" "),
                ),
                SyncState::TrustSync { processed, total } => {
                    let progress = processed * 100 / total;
                    Some(
                        [
                            style("SYNC TRUST").red().to_string(),
                            style(format!("{} ({}%)", processed.separated_string(), progress)).dim().to_string(),
                        ]
                        .join(" "),
                    )
                }
                SyncState::UtxoSync { total, .. } => {
                    Some([style("SYNC UTXO").red().to_string(), style(total.separated_string()).dim().to_string()].join(" "))
                }
                SyncState::UtxoResync => Some([style("SYNC").red().to_string(), style("UTXO").black().to_string()].join(" ")),
                SyncState::NotSynced => Some([style("SYNC").red().to_string(), style("...").black().to_string()].join(" ")),
                SyncState::Synced { .. } => None,
            }
        } else {
            Some(style("SYNC").red().to_string())
        }
    }
}

#[async_trait]
impl Cli for KaspaCli {
    fn init(self: Arc<Self>, term: &Arc<Terminal>) -> TerminalResult<()> {
        *self.term.lock().unwrap() = Some(term.clone());

        self.notifier().try_init()?;

        term.register_event_handler(Arc::new(Box::new(move |event| match event {
            TerminalEvent::Copy | TerminalEvent::Paste => {
                self.notifier().notify(Notification::Clipboard);
            }
        })))?;

        Ok(())
    }

    async fn digest(self: Arc<Self>, term: Arc<Terminal>, cmd: String) -> TerminalResult<()> {
        *self.last_interaction.lock().unwrap() = Instant::now();
        if let Err(err) = self.handlers.execute(&self, &cmd).await {
            term.writeln(style(err.to_string()).red().to_string());
        }
        Ok(())
    }

    async fn complete(self: Arc<Self>, _term: Arc<Terminal>, cmd: String) -> TerminalResult<Option<Vec<String>>> {
        let list = self.handlers.complete(&self, &cmd).await?;
        Ok(list)
    }

    fn prompt(&self) -> Option<String> {
        if self.shutdown.load(Ordering::SeqCst) {
            return Some("halt $ ".to_string());
        }

        let mut prompt = vec![];

        let node_running = if let Some(node) = self.node.lock().unwrap().as_ref() { node.is_running() } else { false };

        let _miner_running = if let Some(miner) = self.miner.lock().unwrap().as_ref() { miner.is_running() } else { false };

        // match (node_running, miner_running) {
        //     (true, true) => prompt.push(style("NM").green().to_string()),
        //     (true, false) => prompt.push(style("N").green().to_string()),
        //     (false, true) => prompt.push(style("M").green().to_string()),
        //     _ => {}
        // }

        if (self.wallet.is_open() && !self.wallet.is_connected()) || (node_running && !self.wallet.is_connected()) {
            prompt.push(style("N/C").red().to_string());
        } else if self.wallet.is_connected() && !self.wallet.is_synced() {
            if let Some(state) = self.sync_state() {
                prompt.push(state);
            }
        }

        if let Some(descriptor) = self.wallet.descriptor() {
            let title = descriptor.title.unwrap_or(descriptor.filename);
            if title.to_lowercase().as_str() != "kaspa" {
                prompt.push(title);
            }

            if let Ok(account) = self.wallet.account() {
                prompt.push(style(account.name_with_id()).blue().to_string());

                if let Ok(balance) = account.balance_as_strings(None) {
                    if let Some(pending) = balance.pending {
                        prompt.push(format!("{} ({})", balance.mature, pending));
                    } else {
                        prompt.push(balance.mature);
                    }
                } else {
                    prompt.push("N/A".to_string());
                }
            }
        }

        prompt.is_not_empty().then(|| prompt.join(" • ") + " $ ")
    }
}

impl cli::Context for KaspaCli {
    fn term(&self) -> Arc<Terminal> {
        self.term.lock().unwrap().as_ref().unwrap().clone()
    }
}

impl KaspaCli {}

#[allow(dead_code)]
async fn select_item<T>(
    term: &Arc<Terminal>,
    prompt: &str,
    argv: &mut Vec<String>,
    iter: impl Stream<Item = Result<Arc<T>>>,
) -> Result<Arc<T>>
where
    T: std::fmt::Display + IdT + Clone + Send + Sync + 'static,
{
    let mut selection = None;
    let list = iter.try_collect::<Vec<_>>().await?;

    if !argv.is_empty() {
        let text = argv.remove(0);
        let matched = list
            .into_iter()
            // - TODO match by name
            .filter(|item| item.id().to_hex().starts_with(&text))
            .collect::<Vec<_>>();

        if matched.len() == 1 {
            return Ok(matched.first().cloned().unwrap());
        } else {
            return Err(Error::MultipleMatches(text));
        }
    }

    while selection.is_none() {
        list.iter().enumerate().for_each(|(seq, item)| {
            term.writeln(format!("{}: {} ({})", seq, item, item.id().to_hex()));
        });

        let text = term.ask(false, &format!("{prompt} ({}..{}) or <enter> to abort: ", 0, list.len() - 1)).await?.trim().to_string();
        if text.is_empty() {
            term.writeln("aborting...");
            return Err(Error::UserAbort);
        } else {
            match text.parse::<usize>() {
                Ok(seq) if seq < list.len() => selection = list.get(seq).cloned(),
                _ => {}
            };
        }
    }

    Ok(selection.unwrap())
}

// async fn select_variant<T>(term: &Arc<Terminal>, prompt: &str, argv: &mut Vec<String>) -> Result<T>
// where
//     T: ToString + DeserializeOwned + Clone + Serialize,
// {
//     if !argv.is_empty() {
//         let text = argv.remove(0);
//         if let Ok(v) = serde_json::from_str::<T>(text.as_str()) {
//             return Ok(v);
//         } else {
//             let accepted = T::list().iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join(", ");
//             return Err(Error::UnrecognizedArgument(text, accepted));
//         }
//     }

//     let mut selection = None;
//     let list = T::list();
//     while selection.is_none() {
//         list.iter().enumerate().for_each(|(seq, item)| {
//             let name = serde_json::to_string(item).unwrap();
//             term.writeln(format!("{}: '{name}' - {}", seq, item.descr()));
//         });

//         let text = term.ask(false, &format!("{prompt} ({}..{}) or <enter> to abort: ", 0, list.len() - 1)).await?.trim().to_string();
//         if text.is_empty() {
//             term.writeln("aborting...");
//             return Err(Error::UserAbort);
//         } else if let Ok(v) = serde_json::from_str::<T>(text.as_str()) {
//             selection = Some(v);
//         } else {
//             match text.parse::<usize>() {
//                 Ok(seq) if seq > 0 && seq < list.len() => selection = list.get(seq).cloned(),
//                 _ => {}
//             };
//         }
//     }

//     Ok(selection.unwrap())
// }

pub async fn kaspa_cli(terminal_options: TerminalOptions, banner: Option<String>) -> Result<()> {
    KaspaCli::init();

    let options = Options::new(terminal_options, None);
    let cli = KaspaCli::try_new_arc(options).await?;

    let banner =
        banner.unwrap_or_else(|| format!("Kaspa Cli Wallet v{} (type 'help' for list of commands)", env!("CARGO_PKG_VERSION")));
    cli.term().writeln(banner);

    // redirect the global log output to terminal
    #[cfg(not(target_arch = "wasm32"))]
    workflow_log::pipe(Some(cli.clone()));

    cli.register_handlers()?;

    // cli starts notification->term trace pipe task
    cli.start().await?;

    // terminal blocks async execution, delivering commands to the terminals
    cli.run().await?;

    // cli stops notification->term trace pipe task
    cli.stop().await?;

    Ok(())
}

mod panic_handler {
    use regex::Regex;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console, js_name="error")]
        pub fn console_error(msg: String);

        type Error;

        #[wasm_bindgen(constructor)]
        fn new() -> Error;

        #[wasm_bindgen(structural, method, getter)]
        fn stack(error: &Error) -> String;
    }

    pub fn process(info: &std::panic::PanicInfo) -> String {
        let mut msg = info.to_string();

        // Add the error stack to our message.
        //
        // This ensures that even if the `console` implementation doesn't
        // include stacks for `console.error`, the stack is still available
        // for the user. Additionally, Firefox's console tries to clean up
        // stack traces, and ruins Rust symbols in the process
        // (https://bugzilla.mozilla.org/show_bug.cgi?id=1519569) but since
        // it only touches the logged message's associated stack, and not
        // the message's contents, by including the stack in the message
        // contents we make sure it is available to the user.

        msg.push_str("\n\nStack:\n\n");
        let e = Error::new();
        let stack = e.stack();

        let regex = Regex::new(r"chrome-extension://[^/]+").unwrap();
        let stack = regex.replace_all(&stack, "");

        msg.push_str(&stack);

        // Safari's devtools, on the other hand, _do_ mess with logged
        // messages' contents, so we attempt to break their heuristics for
        // doing that by appending some whitespace.
        // https://github.com/rustwasm/console_error_panic_hook/issues/7

        msg.push_str("\n\n");

        msg
    }
}

impl KaspaCli {
    pub fn init_panic_hook(self: &Arc<Self>) {
        let this = self.clone();
        let handler = move |info: &std::panic::PanicInfo| {
            let msg = panic_handler::process(info);
            this.term().writeln(msg.crlf());
            panic_handler::console_error(msg);
        };

        std::panic::set_hook(Box::new(handler));

        // #[cfg(target_arch = "wasm32")]
        workflow_log::pipe(Some(self.clone()));
    }
}
