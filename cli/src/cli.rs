use crate::error::Error;
use crate::imports::*;
use crate::modules::*;
use crate::result::Result;
use crate::utils::*;
use async_trait::async_trait;
use cfg_if::cfg_if;
use futures::stream::{Stream, StreamExt, TryStreamExt};
use futures::*;
use kaspa_wallet_core::imports::{AtomicBool, Ordering, ToHex};
use kaspa_wallet_core::runtime::wallet::WalletCreateArgs;
use kaspa_wallet_core::storage::interface::AccessContext;
use kaspa_wallet_core::storage::{AccessContextT, AccountKind, IdT, PrvKeyDataId, PrvKeyDataInfo};
use kaspa_wallet_core::utxo;
use kaspa_wallet_core::{runtime::wallet::AccountCreateArgs, runtime::Wallet, secret::Secret, Events};
use pad::PadStr;
use separator::Separatable;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use workflow_core::channel::*;
use workflow_core::time::Instant;
use workflow_log::*;
use workflow_terminal::*;
pub use workflow_terminal::{parse, Cli, Options as TerminalOptions, Result as TerminalResult, TargetElement as TerminalTarget}; //{CrLf, Terminal};

pub struct KaspaCli {
    term: Arc<Mutex<Option<Arc<Terminal>>>>,
    wallet: Arc<Wallet>,
    notifications_task_ctl: DuplexChannel,
    mute: Arc<AtomicBool>,
    flags: Flags,
    last_interaction: Arc<Mutex<Instant>>,
    handlers: Arc<HandlerCli>,
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
            term.writeln(args.to_string());
            true
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

    pub async fn try_new_arc(options: TerminalOptions) -> Result<Arc<Self>> {
        let wallet = Arc::new(Wallet::try_new(Wallet::local_store()?, None)?);

        let kaspa_cli = Arc::new(KaspaCli {
            term: Arc::new(Mutex::new(None)),
            wallet,
            notifications_task_ctl: DuplexChannel::oneshot(),
            mute: Arc::new(AtomicBool::new(false)),
            flags: Flags::default(),
            last_interaction: Arc::new(Mutex::new(Instant::now())),
            handlers: Arc::new(HandlerCli::default()),
        });

        let term = Arc::new(Terminal::try_new_with_options(kaspa_cli.clone(), options)?);
        term.init().await?;

        Ok(kaspa_cli)
    }

    pub fn term(&self) -> Arc<Terminal> {
        self.term.lock().unwrap().as_ref().cloned().expect("WalletCli::term is not initialized")
    }

    pub fn try_term(&self) -> Option<Arc<Terminal>> {
        self.term.lock().unwrap().as_ref().cloned()
    }

    pub fn wallet(&self) -> Arc<Wallet> {
        self.wallet.clone()
    }

    pub fn store(&self) -> Arc<dyn Interface> {
        self.wallet.store().clone()
    }

    pub fn handler(&self) -> Arc<HandlerCli> {
        self.handlers.clone()
    }

    pub fn flags(&self) -> &Flags {
        &self.flags
    }

    pub fn toggle_mute(&self) -> &'static str {
        utils::toggle(&self.mute)
    }

    pub fn is_mutted(&self) -> bool {
        self.mute.load(Ordering::SeqCst)
    }

    pub fn register_handlers(self: &Arc<Self>) -> Result<()> {
        register_handlers!(
            self,
            self.handlers,
            [
                address,
                // broadcast,
                close,
                connect,
                // create_unsigned_tx,
                create,
                details,
                disconnect,
                estimate,
                exit,
                export,
                halt,
                help,
                hint,
                import,
                info,
                list,
                metrics,
                mute,
                name,
                network,
                new_address,
                open,
                ping,
                reload,
                select,
                send,
                server,
                set,
                // sign,
                // sweep,
                track,
            ]
        );

        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                register_handlers!(
                    self,
                    self.handlers,
                    [
                        reload,
                    ]
                );
            }
        }

        Ok(())
    }

    pub async fn start(self: &Arc<Self>) -> Result<()> {
        self.start_notification_pipe_task();
        self.handlers.start(self).await?;
        // wallet starts rpc and notifier
        self.wallet.start().await?;
        Ok(())
    }

    pub async fn run(self: &Arc<Self>) -> Result<()> {
        self.term().run().await?;
        Ok(())
    }

    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.handlers.stop(&self).await?;
        // wallet stops the notifier
        self.wallet.stop().await?;
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
                        // if let Ok(msg) = msg {
                        //     let text = format!("{msg:#?}").replace('\n',"\r\n");
                        //     println!("#### text: {text:?}");
                        //     term.pipe_crlf.send(text).await.unwrap_or_else(|err|log_error!("WalletCli::notification_pipe_task() unable to route to term: `{err}`"));
                        // }
                        break;
                    },
                    // msg = notification_channel_receiver.recv().fuse() => {
                    //     if let Ok(msg) = msg {

                    //         log_info!("Received RPC notification: {msg:#?}");
                    //         let text = format!("{msg:#?}").crlf();//replace('\n',"\r\n"); //.payload);
                    //         term.pipe_crlf.send(text).await.unwrap_or_else(|err|log_error!("WalletCli::notification_pipe_task() unable to route to term: `{err}`"));
                    //     }
                    // },

                    msg = multiplexer.receiver.recv().fuse() => {

                        if let Ok(msg) = msg {
                            match msg {
                                Events::Connect(_url) => {
                                    // log_info!("Connected to {url}");
                                },
                                Events::Disconnect(url) => {
                                    tprintln!(this, "Disconnected from {url}");
                                },
                                Events::UtxoIndexNotEnabled => {
                                    tprintln!(this, "Error: Kaspa node UTXO index is not enabled...")
                                },
                                Events::ServerStatus {
                                    is_synced,
                                    server_version,
                                    url,
                                    // has_utxo_index,
                                    ..
                                } => {

                                    tprintln!(this, "Connected to Kaspa node version {server_version} at {url}\n");


                                    let is_open = this.wallet.is_open().unwrap_or_else(|err| { terrorln!(this, "Unable to check if wallet is open: {err}"); false });

                                    if !is_synced {
                                        if is_open {
                                            terrorln!(this, "Error: Unable to sync wallet - Kaspa node is not synced...");

                                        } else {
                                            terrorln!(this, "Error: Kaspa node is not synced...");
                                        }
                                    }
                                },
                                Events::WalletHasLoaded {
                                    hint
                                } => {

                                    if let Some(hint) = hint {
                                        tprintln!(this, "\nYour wallet hint is: {hint}\n");
                                    }

                                    this.list().await.unwrap_or_else(|err|terrorln!(this, "{err}"));
                                },
                                Events::UtxoProcessor(event) => {

                                    match event {

                                        utxo::Events::DAAScoreChange(daa) => {
                                            if this.is_mutted() && this.flags.get(Track::Daa) {
                                                tprintln!(this, "DAAScoreChange: {daa}");
                                            }
                                        },
                                        utxo::Events::Pending {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this, "pending {tx}");
                                            }
                                        },
                                        utxo::Events::Reorg {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this, "pending {tx}");
                                            }
                                        },
                                        utxo::Events::External {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this,"external {tx}");
                                            }
                                        },
                                        utxo::Events::Maturity {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this,"maturity {tx}");
                                            }
                                        },
                                        utxo::Events::Debit {
                                            record
                                        } => {
                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                                let tx = record.format(&this.wallet);
                                                tprintln!(this,"{tx}");
                                            }
                                        },
                                        utxo::Events::Balance {
                                            balance,
                                            id,
                                            mature_utxo_size,
                                            pending_utxo_size,
                                        } => {

                                            if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Balance)) {
                                                let network_type = this.wallet.network().expect("missing network type");
                                                let balance = BalanceStrings::from((&balance,&network_type, Some(19)));
                                                let id = id.short();

                                                let pending_utxo_info = if pending_utxo_size > 0 {
                                                    format!("({pending_utxo_size} pending)")
                                                } else { "".to_string() };
                                                let utxo_info = style(format!("{} UTXOs {pending_utxo_info}", mature_utxo_size.separated_string())).dim();

                                                tprintln!(this, "{} {id}: {balance}   {utxo_info}",style("balance".pad_to_width(8)).blue());
                                            }
                                        },
                                    }
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

    pub(crate) async fn create_wallet(&self, name: Option<&str>) -> Result<()> {
        let term = self.term();

        if self.wallet.exists(name).await? {
            tprintln!(self, "WARNING - A previously created wallet already exists!");

            let overwrite = term
                .ask(false, "Are you sure you want to overwrite it (type 'y' to approve)?: ")
                .await?
                .trim()
                .to_string()
                .to_lowercase();
            if overwrite.ne("y") {
                return Ok(());
            }
        }

        let account_title = term.ask(false, "Default account title: ").await?.trim().to_string();
        let account_name = account_title.replace(' ', "-").to_lowercase();

        tpara!(
            self,
            "\n\
        \"Phishing hint\" is a secret word or a phrase that is displayed \
        when you open your wallet. If you do not see the hint when opening \
        your wallet, you may be accessing a fake wallet designed to steal \
        your private key. If this occurs, stop using the wallet immediately, \
        check the browser URL domain name and seek help on social networks \
        (Kaspa Discord or Telegram). \
        \n\
        ",
        );

        let hint = term.ask(false, "Create phishing hint (optional, press <enter> to skip): ").await?.trim().to_string();
        let hint = if hint.is_empty() { None } else { Some(hint) };

        let wallet_secret = Secret::new(term.ask(true, "Enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
        if wallet_secret.as_ref().is_empty() {
            return Err(Error::WalletSecretRequired);
        }
        let wallet_secret_validate =
            Secret::new(term.ask(true, "Re-enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
        if wallet_secret_validate.as_ref() != wallet_secret.as_ref() {
            return Err(Error::WalletSecretMatch);
        }

        tprintln!(self, "");
        tpara!(
            self,
            "\
            PLEASE NOTE: The optional payment password, if provided, will be required to \
            issue transactions. This password will also be required when recovering your wallet \
            in addition to your private key or mnemonic. If you loose this password, you will not \
            be able to use mnemonic to recover your wallet! \
            ",
        );

        let payment_secret = term.ask(true, "Enter payment password (optional): ").await?;
        let payment_secret =
            if payment_secret.trim().is_empty() { None } else { Some(Secret::new(payment_secret.trim().as_bytes().to_vec())) };

        if let Some(payment_secret) = payment_secret.as_ref() {
            let payment_secret_validate = Secret::new(
                term.ask(true, "Enter payment (private key encryption) password (optional): ").await?.trim().as_bytes().to_vec(),
            );
            if payment_secret_validate.as_ref() != payment_secret.as_ref() {
                return Err(Error::PaymentSecretMatch);
            }
        }

        // suspend commits for multiple operations
        self.wallet.store().batch().await?;

        let account_kind = AccountKind::Bip32;
        let wallet_args = WalletCreateArgs::new(None, hint, wallet_secret.clone(), true);
        let prv_key_data_args = PrvKeyDataCreateArgs::new(None, wallet_secret.clone(), payment_secret.clone());
        let account_args = AccountCreateArgs::new(account_name, account_title, account_kind, wallet_secret.clone(), payment_secret);
        let descriptor = self.wallet.create_wallet(wallet_args).await?;
        let (prv_key_data_id, mnemonic) = self.wallet.create_prv_key_data(prv_key_data_args).await?;
        let account = self.wallet.create_bip32_account(prv_key_data_id, account_args).await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        self.wallet.store().flush(&ctx).await?;

        ["", "---", "", "IMPORTANT:", ""].into_iter().for_each(|line| term.writeln(line));

        tpara!(
            self,
            "Your mnemonic phrase allows your to re-create your private key. \
            The person who has access to this mnemonic will have full control of \
            the Kaspa stored in it. Keep your mnemonic safe. Write it down and \
            store it in a safe, preferably in a fire-resistant location. Do not \
            store your mnemonic on this computer or a mobile device. This wallet \
            will never ask you for this mnemonic phrase unless you manually \
            initial a private key recovery. \
            ",
        );

        // descriptor

        ["", "Never share your mnemonic with anyone!", "---", "", "Your default wallet account mnemonic:", mnemonic.phrase()]
            .into_iter()
            .for_each(|line| term.writeln(line));

        term.writeln("");
        if let Some(descriptor) = descriptor {
            term.writeln(format!("Your wallet is stored in: {}", descriptor));
            term.writeln("");
        }

        term.writeln("");
        let receive_address = account.receive_address().await?;
        term.writeln(format!("Your default account deposit address: {}", receive_address));

        Ok(())
    }

    pub(crate) async fn create_account(
        &self,
        prv_key_data_id: PrvKeyDataId,
        account_kind: AccountKind,
        name: Option<&str>,
    ) -> Result<()> {
        let term = self.term();

        if matches!(account_kind, AccountKind::MultiSig) {
            return Err(Error::Custom(
                "MultiSig accounts are not currently supported (will be available in the future version)".to_string(),
            ));
        }

        let (title, name) = if let Some(name) = name {
            (name.to_string(), name.to_string())
        } else {
            let title = term.ask(false, "Please enter account title (optional, press <enter> to skip): ").await?.trim().to_string();
            let name = title.replace(' ', "-").to_lowercase();
            (title, name)
        };

        let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
        if wallet_secret.as_ref().is_empty() {
            return Err(Error::WalletSecretRequired);
        }

        let prv_key_info = self.wallet.store().as_prv_key_data_store()?.load_key_info(&prv_key_data_id).await?;
        if let Some(keyinfo) = prv_key_info {
            let payment_secret = if keyinfo.is_encrypted() {
                let payment_secret = Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec());
                if payment_secret.as_ref().is_empty() {
                    return Err(Error::PaymentSecretRequired);
                } else {
                    Some(payment_secret)
                }
            } else {
                None
            };

            let account_args = AccountCreateArgs::new(name, title, account_kind, wallet_secret, payment_secret);
            let account = self.wallet.create_bip32_account(prv_key_data_id, account_args).await?;

            tprintln!(self, "\naccount created: {}\n", account.get_list_string()?);
            self.wallet.select(Some(&account)).await?;
        } else {
            return Err(Error::KeyDataNotFound);
        }

        Ok(())
    }

    pub async fn account(&self) -> Result<Arc<runtime::Account>> {
        if let Ok(account) = self.wallet.account() {
            Ok(account)
        } else {
            let account = self.select_account().await?;
            self.wallet.select(Some(&account)).await?;
            Ok(account)
        }
    }

    pub async fn prompt_account(&self) -> Result<Arc<runtime::Account>> {
        self.select_account_with_args(false).await
    }

    pub async fn select_account(&self) -> Result<Arc<runtime::Account>> {
        self.select_account_with_args(true).await
    }

    async fn select_account_with_args(&self, autoselect: bool) -> Result<Arc<runtime::Account>> {
        let mut selection = None;

        let mut list_by_key = Vec::<(Arc<PrvKeyDataInfo>, Vec<(usize, Arc<runtime::Account>)>)>::new();
        let mut flat_list = Vec::<Arc<runtime::Account>>::new();

        let mut keys = self.wallet.keys().await?;
        while let Some(key) = keys.try_next().await? {
            let mut prv_key_accounts = Vec::new();
            let mut accounts = self.wallet.accounts(Some(key.id)).await?;
            while let Some(account) = accounts.next().await {
                let account = account?;
                prv_key_accounts.push((flat_list.len(), account.clone()));
                flat_list.push(account.clone());
            }

            list_by_key.push((key.clone(), prv_key_accounts));
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
                    tprintln!(self, "    {seq}: {}", account.get_list_string().unwrap_or_else(|err| panic!("{err}")));
                })
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
        let ident = account.name_or_id();
        tprintln!(self, "\nselecting account: {ident}\n");

        Ok(account)
    }

    async fn list(&self) -> Result<()> {
        let mut keys = self.wallet.keys().await?;

        tprintln!(self);
        while let Some(key) = keys.try_next().await? {
            tprintln!(self, "• {key}");
            let mut accounts = self.wallet.accounts(Some(key.id)).await?;
            while let Some(account) = accounts.try_next().await? {
                let receive_address = account.receive_address().await?;
                tprintln!(self, "    • {}", account.get_list_string()?);
                tprintln!(self, "      {}", style(receive_address.to_string()).yellow());
            }
        }
        tprintln!(self);

        Ok(())
    }
}

#[async_trait]
impl Cli for KaspaCli {
    fn init(&self, term: &Arc<Terminal>) -> TerminalResult<()> {
        *self.term.lock().unwrap() = Some(term.clone());
        Ok(())
    }

    async fn digest(self: Arc<Self>, _term: Arc<Terminal>, cmd: String) -> TerminalResult<()> {
        *self.last_interaction.lock().unwrap() = Instant::now();
        self.handlers.execute(&self, &cmd).await?;
        Ok(())
    }

    async fn complete(self: Arc<Self>, _term: Arc<Terminal>, cmd: String) -> TerminalResult<Option<Vec<String>>> {
        let list = self.handlers.complete(&self, &cmd).await?;
        Ok(list)
    }

    fn prompt(&self) -> Option<String> {
        if let Some(name) = self.wallet.name() {
            if let Ok(account) = self.wallet.account() {
                let ident = account.name_or_id();
                Some(format!("{name} • {ident} $ "))
            } else {
                Some(format!("{name} $ "))
            }
        } else {
            None
        }
    }
}

impl cli::Context for KaspaCli {
    fn term(&self) -> Arc<Terminal> {
        self.term.lock().unwrap().as_ref().unwrap().clone()
    }
}

impl KaspaCli {}

use kaspa_wallet_core::runtime::{self, BalanceStrings, PrvKeyDataCreateArgs};

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

pub async fn kaspa_cli(options: TerminalOptions, banner: Option<String>) -> Result<()> {

    KaspaCli::init();

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
