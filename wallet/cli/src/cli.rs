use crate::actions::*;
use crate::error::Error;
use crate::helpers;
use crate::result::Result;
use crate::utils::*;
use cfg_if::cfg_if;
use kaspa_wallet_core::storage::interface::AccessContext;
use pad::PadStr;
// use crate::settings::Settings;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt, TryStreamExt};
use futures::*;
use kaspa_consensus_core::networktype::NetworkType;
use kaspa_wallet_core::accounts::gen0::import::*;
use kaspa_wallet_core::imports::{AtomicBool, Ordering, ToHex};
use kaspa_wallet_core::runtime::wallet::WalletCreateArgs;
use kaspa_wallet_core::storage::{AccessContextT, AccountKind, IdT, PrvKeyDataId, PrvKeyDataInfo};
use kaspa_wallet_core::tx::PaymentOutputs;
use kaspa_wallet_core::{runtime::wallet::AccountCreateArgs, runtime::Wallet, secret::Secret};
use kaspa_wallet_core::{Address, ConnectOptions, ConnectStrategy, Events, Settings};
// use kaspa_wrpc_client::WrpcEncoding;
// use serde::de::DeserializeOwned;
// use serde::Serialize;
use std::sync::{Arc, Mutex};
use textwrap::wrap;
use workflow_core::abortable::Abortable;
use workflow_core::channel::*;
// use workflow_core::enums::EnumTrait;
use workflow_core::runtime as application_runtime;
// use workflow_dom::utils::window;
use separator::Separatable;
use workflow_log::*;
use workflow_terminal::*;
pub use workflow_terminal::{parse, Cli, Options as TerminalOptions, Result as TerminalResult, TargetElement as TerminalTarget}; //{CrLf, Terminal};

struct WalletCli {
    term: Arc<Mutex<Option<Arc<Terminal>>>>,
    wallet: Arc<Wallet>,
    notifications_task_ctl: DuplexChannel,
    // ---
    mute: Arc<AtomicBool>,
    flags: Flags,
    // track_balance : Arc<AtomicBool>,
    // track_pending : Arc<AtomicBool>,
    // track_utxo : Arc<AtomicBool>,
    // track_daa : Arc<AtomicBool>,
}

impl workflow_log::Sink for WalletCli {
    fn write(&self, _target: Option<&str>, _level: Level, args: &std::fmt::Arguments<'_>) -> bool {
        if let Some(term) = self.term() {
            term.writeln(args.to_string());
            true
        } else {
            false
        }
    }
}

impl WalletCli {
    fn new(wallet: Arc<Wallet>) -> Self {
        WalletCli {
            term: Arc::new(Mutex::new(None)),
            wallet,
            notifications_task_ctl: DuplexChannel::oneshot(),
            mute: Arc::new(AtomicBool::new(false)),
            flags: Flags::new(),
        }
    }

    fn term(&self) -> Option<Arc<Terminal>> {
        self.term.lock().unwrap().as_ref().cloned()
    }

    pub fn is_mutted(&self) -> bool {
        self.mute.load(Ordering::SeqCst)
    }

    async fn action(&self, action: Action, mut argv: Vec<String>, term: Arc<Terminal>) -> Result<()> {
        argv.remove(0);

        match action {
            Action::Help => {
                term.writeln("\n\rCommands:\n\r");
                display_help(&term);
            }
            Action::Halt => {
                panic!("halting...");
            }
            Action::Exit => {
                term.writeln("bye!");
                #[cfg(not(target_arch = "wasm32"))]
                term.exit().await;
                #[cfg(target_arch = "wasm32")]
                workflow_dom::utils::window().location().reload().ok();
            }
            Action::Set => {
                if argv.is_empty() {
                    term.writeln("\n\rSettings:\n\r");

                    let list = Settings::list();

                    // let settings = self.wallet.settings().iter();

                    // let len = commands.iter().map(|(c, _)| c.len()).fold(0, |a, b| a.max(b));

                    let list = list
                        .iter()
                        .map(|setting| {
                            let value: String = self.wallet.settings().get(setting.clone()).unwrap_or_else(|| "-".to_string());
                            let descr = setting.descr();
                            (setting.to_lowercase_string(), value, descr)
                        })
                        .collect::<Vec<(_, _, _)>>();
                    let c1 = list.iter().map(|(c, _, _)| c.len()).fold(0, |a, b| a.max(b)) + 4;
                    let c2 = list.iter().map(|(_, c, _)| c.len()).fold(0, |a, b| a.max(b)) + 4;

                    list.iter().for_each(|(k, v, d)| {
                        term.writeln(format!(
                            "{}: {} \t {}",
                            k.pad_to_width_with_alignment(c1, pad::Alignment::Right),
                            v.pad_to_width(c2),
                            d
                        ));
                    });

                    // for setting in list {
                    //     let value : String = self.wallet.settings().get(setting.clone()).unwrap_or_default();
                    //     let descr = setting.descr();
                    //     term.writeln(format!("\t{}:\t{}\t{}", setting.to_lowercase_string(), value, descr));
                    //     //.map(|v|v.to_string()).unwrap_or_else(|| "-".to_string())));
                    // }
                    // term.writeln(format!("network : {}", settings.network.map(|v|v.to_string()).unwrap_or_else(|| "-".to_string())));
                    // term.writeln(format!("server : {}", settings.server.unwrap_or_else(|| "-".to_string())));
                    // term.writeln(format!("wallet : {}", settings.wallet.unwrap_or_else(|| "-".to_string())));
                    // - TODO use Store to load settings
                } else if argv.len() != 2 {
                    term.writeln("\n\rError:\n\r");
                    term.writeln("Usage:\n\rset <key> <value>");
                    return Ok(());
                } else {
                    let key = argv[0].as_str();
                    let value = argv[1].as_str().trim();

                    if value.contains(' ') || value.contains('\t') {
                        return Err(Error::Custom("Whitespace in settings is not allowed".to_string()));
                    }

                    match key {
                        "network" => {
                            let network: NetworkType = value.parse().map_err(|_| "Unknown network type".to_string())?;
                            self.wallet.settings().set(Settings::Network, network).await?;
                        }
                        "server" => {
                            self.wallet.settings().set(Settings::Server, value).await?;
                        }
                        "wallet" => {
                            self.wallet.settings().set(Settings::Wallet, value).await?;
                        }
                        _ => return Err(Error::Custom(format!("Unknown setting '{}'", key))),
                    }
                    self.wallet.settings().try_store().await?;
                }
            }
            Action::Mute => {
                log_info!("mute is {}", toggle(&self.mute));
            }
            Action::Track => {
                if let Some(attr) = argv.first() {
                    let track: Track = attr.parse()?;
                    self.flags.toggle(track);
                } else {
                    for flag in self.flags.map().iter() {
                        let k = flag.key().to_string();
                        let v = flag.value().load(Ordering::SeqCst);
                        let s = if v { "on" } else { "off" };
                        term.writeln(format!("{k} is {s}"));
                    }
                }
            }
            Action::Connect => {
                let url = argv.first().cloned().or_else(|| self.wallet.settings().get(Settings::Server));

                let network_type = self.wallet.network()?;
                let url = self.wallet.rpc_client().parse_url(url, network_type)?;
                term.writeln(format!("Connecting to {}...", url.clone().unwrap_or_else(|| "default".to_string())));

                let options = ConnectOptions { block_async_connect: true, strategy: ConnectStrategy::Fallback, url };
                self.wallet.rpc_client().connect(options).await?;
            }
            Action::Disconnect => {
                self.wallet.rpc_client().shutdown().await?;
            }
            Action::GetInfo => {
                let response = self.wallet.get_info().await?;
                term.writeln(response);
            }
            Action::Metrics => {
                let response = self.wallet.rpc().get_metrics(true, true).await.map_err(|e| e.to_string())?;
                term.writeln(format!("{:#?}", response));
            }
            Action::Ping => {
                if self.wallet.ping().await {
                    term.writeln("ping ok");
                } else {
                    term.writeln("ping error");
                }
            }
            // Action::Balance => {}
            //     let accounts = self.wallet.accounts();
            //     for account in accounts {
            //         let balance = account.balance();
            //         let name = account.name();
            //         log_info!("{name} - {balance} KAS");
            //     }
            // }
            Action::Create => {
                let is_open = self.wallet.is_open()?;

                let op = if argv.is_empty() { if is_open { "account" } else { "wallet" }.to_string() } else { argv.remove(0) };

                match op.as_str() {
                    "wallet" => {
                        let wallet_name = if argv.is_empty() {
                            None
                        } else {
                            let name = argv.remove(0);
                            let name = name.trim().to_string();

                            Some(name)
                        };

                        let wallet_name = wallet_name.as_deref();
                        self.create_wallet(wallet_name, term).await?;
                    }
                    "account" => {
                        if !is_open {
                            return Err(Error::WalletIsNotOpen);
                        }
                        //- TODO
                        //- TODO
                        //- TODO
                        //- TODO
                        // let account_kind: AccountKind = select_variant(&term, "Please select account type", &mut argv).await?;

                        // let prv_key_data_info =
                        //     select_item(&term, "Please select private key", &mut argv, self.wallet.keys().await?.err_into()).await?;

                        // self.create_account(prv_key_data_info.id, account_kind, term).await?;
                    }
                    _ => {
                        term.writeln("\n\rError:\n\r");
                        term.writeln("Usage:\n\rcreate <account|wallet>");
                        return Ok(());
                    }
                }
            }
            Action::Network => {
                if let Some(network_type) = argv.first() {
                    let network_type: NetworkType =
                        network_type.trim().parse::<NetworkType>().map_err(|_| "Unknown network type: `{network_type}`")?;
                    // .map_err(|err|err.to_string())?;
                    term.writeln(format!("Setting network type to: {network_type}"));
                    self.wallet.select_network(network_type)?;
                    self.wallet.settings().set(Settings::Network, network_type).await?;
                    // self.wallet.settings().try_store().await?;
                } else {
                    let network_type = self.wallet.network()?;
                    term.writeln(format!("Current network type is: {network_type}"));
                }
            }
            Action::Server => {
                if let Some(url) = argv.first() {
                    self.wallet.settings().set(Settings::Server, url).await?;
                    term.writeln(format!("Setting RPC server to: {url}"));
                } else {
                    let server = self.wallet.settings().get(Settings::Server).unwrap_or_else(|| "n/a".to_string());
                    term.writeln(format!("Current RPC server is: {server}"));
                }
            }
            Action::Broadcast => {
                self.wallet.broadcast().await?;
            }
            Action::CreateUnsignedTx => {
                let account = self.wallet.account()?;
                account.create_unsigned_transaction().await?;
            }
            Action::DumpUnencrypted => {
                let account = self.wallet.account()?;
                let password = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                let mut _payment_secret = Option::<Secret>::None;

                if self.wallet.is_account_key_encrypted(&account).await?.is_some_and(|flag| flag) {
                    _payment_secret = Some(Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()));
                }
                let keydata = self.wallet.get_prv_key_data(password.clone(), &account.prv_key_data_id).await?;
                if keydata.is_none() {
                    return Err("It is read only wallet.".into());
                }

                todo!();

                // let (mnemonic, xprv) = self.wallet.dump_unencrypted(account, password, payment_secret).await?;
                // term.writeln(format!("mnemonic: {mnemonic}"));
                // term.writeln(format!("xprv: {xprv}"));
            }
            Action::NewAddress => {
                let account = self.wallet.account()?;
                let response = account.new_receive_address().await?;
                term.writeln(response);
            }
            // Action::Parse => {
            //     self.wallet.parse().await?;
            // }
            Action::Send => {
                // address, amount, priority fee
                let account = self.wallet.account()?;

                if argv.len() < 2 {
                    return Err("Usage: send <address> <amount> <priority fee>".into());
                }

                let address = argv.get(0).unwrap();
                let amount = argv.get(1).unwrap();
                let priority_fee = argv.get(2);

                let priority_fee_sompi = if let Some(fee) = priority_fee { Some(helpers::kas_str_to_sompi(fee)?) } else { None };

                let address = serde_json::from_str::<Address>(address)?;
                let amount_sompi = helpers::kas_str_to_sompi(amount)?;

                let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                let mut payment_secret = Option::<Secret>::None;

                if self.wallet.is_account_key_encrypted(&account).await?.is_some_and(|f| f) {
                    payment_secret = Some(Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()));
                }
                let keydata = self.wallet.get_prv_key_data(wallet_secret.clone(), &account.prv_key_data_id).await?;
                if keydata.is_none() {
                    return Err("It is read only wallet.".into());
                }
                let abortable = Abortable::default();

                let outputs = PaymentOutputs::try_from((address.clone(), amount_sompi))?;
                let ids =
                    // account.send(&address, amount_sompi, priority_fee_sompi, keydata.unwrap(), payment_secret, &abortable).await?;
                    account.send(&outputs, priority_fee_sompi, false, wallet_secret, payment_secret, &abortable).await?;

                term.writeln(format!("\r\nSending {amount} KAS to {address}, tx ids:"));
                term.writeln(format!("{}\r\n", ids.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\r\n")));
            }
            Action::Address => {
                let address = self.wallet.account()?.receive_address().await?.to_string();
                term.writeln(address);
            }
            Action::ShowAddresses => {
                let manager = self.wallet.account()?.receive_address_manager()?;
                let index = manager.index()?;
                let addresses = manager.get_range_with_args(0..index, false).await?;
                term.writeln(format!("Receive addresses: 0..{index}"));
                term.writeln(format!("{}\r\n", addresses.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\r\n")));

                let manager = self.wallet.account()?.change_address_manager()?;
                let index = manager.index()?;
                let addresses = manager.get_range_with_args(0..index, false).await?;
                term.writeln(format!("Change addresses: 0..{index}"));
                term.writeln(format!("{}\r\n", addresses.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\r\n")));
            }
            Action::Sign => {
                self.wallet.account()?.sign().await?;
            }
            Action::Sweep => {
                self.wallet.account()?.sweep().await?;
            }
            Action::SubscribeDaaScore => {
                self.wallet.subscribe_daa_score().await?;
            }
            Action::UnsubscribeDaaScore => {
                self.wallet.unsubscribe_daa_score().await?;
            }

            // ~~~
            Action::Import => {
                if argv.is_empty() || argv.get(0) == Some(&"help".to_string()) {
                    log_info!("Usage: import [mnemonic]");
                    return Ok(());
                }

                let what = argv.get(0).unwrap();
                match what.as_str() {
                    "mnemonic" => {
                        let mnemonic = helpers::ask_mnemonic(&term).await?;
                        log_info!("Mnemonic: {:?}", mnemonic);
                    }
                    "legacy" => {
                        if exists_v0_keydata().await? {
                            let import_secret = Secret::new(
                                term.ask(true, "Enter the password for the wallet you are importing:")
                                    .await?
                                    .trim()
                                    .as_bytes()
                                    .to_vec(),
                            );
                            let wallet_secret =
                                Secret::new(term.ask(true, "Enter wallet password:").await?.trim().as_bytes().to_vec());
                            self.wallet.import_gen0_keydata(import_secret, wallet_secret).await?;
                        } else if application_runtime::is_web() {
                            return Err("'kaspanet' web wallet storage not found at this domain name".into());
                        } else {
                            return Err("KDX/kaspanet keydata file not found".into());
                        }
                    }
                    "kaspa-wallet" => {}
                    _ => {
                        return Err(format!("Invalid argument: {}", what).into());
                    }
                }
            }

            // ~~~
            Action::List => {
                let mut keys = self.wallet.keys().await?;
                while let Some(key) = keys.try_next().await? {
                    term.writeln(format!("{key}"));
                    let mut accounts = self.wallet.accounts(Some(key.id)).await?;
                    while let Some(account) = accounts.try_next().await? {
                        term.writeln(format!("    {}", account.get_ls_string()?));
                        // term.writeln(format!("    {}", account.get_ls_string()?));
                    }
                }
            }
            Action::Select => {
                if argv.is_empty() {
                    self.wallet.select(None).await?;
                } else {
                    // let name = argv.remove(0);
                    // let account = {
                    //     // let accounts = ;
                    //     self.wallet
                    //         .active_accounts()
                    //         .inner()
                    //         .values()
                    //         .find(|account| account.name() == name)
                    //         .ok_or(Error::AccountNotFound(name))?
                    //         .clone()
                    // };

                    let account = select_account(&term, &self.wallet).await?;
                    self.wallet.select(Some(account)).await?;
                }
            }
            Action::Open => {
                // let mut prefix = AddressPrefix::Mainnet;
                // if argv.contains(&"testnet".to_string()) {
                //     prefix = AddressPrefix::Testnet;
                // } else if argv.contains(&"simnet".to_string()) {
                //     prefix = AddressPrefix::Simnet;
                // } else if argv.contains(&"devnet".to_string()) {
                //     prefix = AddressPrefix::Devnet;
                // }

                // let name = if let Some(name) = argv.first().cloned() {
                //     Some(name)
                // } else if let Some(name) = self.wallet.settings().inner().wallet.clone() {
                //     Some(name)
                // } else {
                //     None
                // };

                let secret = Secret::new(term.ask(true, "Enter wallet password:").await?.trim().as_bytes().to_vec());
                self.wallet.load(secret, None).await?;
            }
            Action::Close => {
                self.wallet.reset().await?;
            }

            #[cfg(target_arch = "wasm32")]
            Action::Reload => {
                workflow_dom::utils::window().location().reload().ok();
            }
        }

        Ok(())
    }

    async fn start(self: &Arc<Self>) -> Result<()> {
        self.notification_pipe_task();
        Ok(())
    }

    async fn stop(self: &Arc<Self>) -> Result<()> {
        self.notifications_task_ctl.signal(()).await?;
        Ok(())
    }

    pub fn notification_pipe_task(self: &Arc<Self>) {
        let this = self.clone();
        let _term = self.term().unwrap_or_else(|| panic!("WalletCli::notification_pipe_task(): `term` is not initialized"));
        // let notification_channel_receiver = self.wallet.rpc_client().notification_channel_receiver();
        let multiplexer = MultiplexerChannel::from(self.wallet.multiplexer());
        workflow_core::task::spawn(async move {
            // term.writeln(args.to_string());
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
                                    log_info!("Disconnected from {url}");
                                },
                                Events::UtxoIndexNotEnabled => {
                                    log_error!("Error: Kaspa node UTXO index is not enabled...")
                                },
                                Events::ServerStatus {
                                    is_synced,
                                    server_version,
                                    url,
                                    // has_utxo_index,
                                    ..
                                } => {

                                    log_info!("Connected to Kaspa node version {server_version} at {url}");


                                    // log_info!("Server version server {server_version}");
                                    if !is_synced {
                                        let is_open = this.wallet.is_open().unwrap_or_else(|err| { log_error!("Unable to check if wallet is open: {err}"); false });
                                        if is_open {
                                            log_error!("Error: Unable to sync wallet - Kaspa node is not synced...");

                                        } else {
                                            log_error!("Error: Kaspa node is not synced...");
                                        }
                                    }
                                },
                                Events::DAAScoreChange(daa) => {
                                    if this.is_mutted() && this.flags.get(Track::Daa) {
                                        log_info!("DAAScoreChange: {daa}");
                                    }
                                },
                                Events::Credit {
                                    record
                                } => {
                                    if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                        let tx = record.format(&this.wallet);
                                        log_info!("{tx}");
                                    }
                                },
                                Events::Debit {
                                    record
                                } => {
                                    if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Utxo)) {
                                        let tx = record.format(&this.wallet);
                                        log_info!("{tx}");
                                    }
                                },
                                Events::Balance {
                                    balance,
                                    account_id,
                                    mature_utxo_size,
                                    pending_utxo_size,
                                } => {
                                    if !this.is_mutted() || (this.is_mutted() && this.flags.get(Track::Balance)) {
                                        let network_type = this.wallet.network().expect("missing network type");
                                        let balance = BalanceStrings::from((&balance,&network_type, Some(19)));
                                        let account_id = account_id.short();

                                        let pending_utxo_info = if pending_utxo_size > 0 {
                                            format!("({pending_utxo_size} pending)")
                                        } else { "".to_string() };
                                        let utxo_info = style(format!("{} UTXOs {pending_utxo_info}", mature_utxo_size.separated_string())).dim();

                                        log_info!("{} {account_id}: {balance} {utxo_info}",style("balance".pad_to_width(8)).blue());
                                    }
                                },
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

    async fn create_wallet(&self, name: Option<&str>, term: Arc<Terminal>) -> Result<()> {
        use kaspa_wallet_core::error::Error;

        if self.wallet.exists(name).await? {
            term.writeln("WARNING - A previously created wallet already exists!");

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

        log_info!("");
        log_info!("\"Phishing hint\" is a secret word or a phrase that is displayed when you open your wallet.");
        log_info!("If you do not see the hint when opening your wallet, you may be accessing a fake wallet designed to steal your private key.");
        log_info!("If this occurs, stop using the wallet immediately, check the domain name and seek help on social networks (Kaspa Discord or Telegram).");
        log_info!("");
        let hint = term.ask(false, "Create phishing hint (optional, press <enter> to skip): ").await?.trim().to_string();
        let hint = if hint.is_empty() { None } else { Some(hint) };

        let wallet_secret = Secret::new(term.ask(true, "Enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
        if wallet_secret.as_ref().is_empty() {
            return Err(Error::WalletSecretRequired.into());
        }
        let wallet_secret_validate =
            Secret::new(term.ask(true, "Re-enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
        if wallet_secret_validate.as_ref() != wallet_secret.as_ref() {
            return Err(Error::WalletSecretMatch.into());
        }

        log_info!("");
        wrap(
            "PLEASE NOTE: The optional payment password, if provided, will be required to \
            issue transactions. This password will also be required when recovering your wallet \
            in addition to your private key or mnemonic. If you loose this password, you will not \
            be able to use mnemonic to recover your wallet!",
            70,
        )
        .into_iter()
        .for_each(|line| term.writeln(line));
        log_info!("");

        let payment_secret = term.ask(true, "Enter payment password (optional): ").await?;
        // let payment_secret = payment_secret.trim();
        let payment_secret =
            if payment_secret.trim().is_empty() { None } else { Some(Secret::new(payment_secret.trim().as_bytes().to_vec())) };

        // let payment_secret = Secret::new(
        //     .as_bytes().to_vec(),
        // );
        if let Some(payment_secret) = payment_secret.as_ref() {
            let payment_secret_validate = Secret::new(
                term.ask(true, "Enter payment (private key encryption) password (optional): ").await?.trim().as_bytes().to_vec(),
            );
            if payment_secret_validate.as_ref() != payment_secret.as_ref() {
                return Err(Error::PaymentSecretMatch.into());
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

        wrap(
            "Your mnemonic phrase allows your to re-create your private key. \
            The person who has access to this mnemonic will have full control of \
            the Kaspa stored in it. Keep your mnemonic safe. Write it down and \
            store it in a safe, preferably in a fire-resistant location. Do not \
            store your mnemonic on this computer or a mobile device. This wallet \
            will never ask you for this mnemonic phrase unless you manually \
            initial a private key recovery.",
            70,
        )
        .into_iter()
        .for_each(|line| term.writeln(line));

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

    async fn _create_account(&self, _prv_key_data_id: PrvKeyDataId, account_kind: AccountKind, term: Arc<Terminal>) -> Result<()> {
        use kaspa_wallet_core::error::Error;

        if matches!(account_kind, AccountKind::MultiSig) {
            return Err(Error::Custom(
                "MultiSig accounts are not currently supported (will be available in the future version)".to_string(),
            )
            .into());
        }

        let title = term.ask(false, "Account title: ").await?.trim().to_string();
        let name = title.replace(' ', "-").to_lowercase();

        let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
        if wallet_secret.as_ref().is_empty() {
            return Err(Error::WalletSecretRequired.into());
        }

        let payment_password = Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec());

        // let account_kind = AccountKind::Bip32;
        let _account_args = AccountCreateArgs::new(name, title, account_kind, wallet_secret, Some(payment_password));

        // self.wallet.create_bip32_account(wallet_secret, payment_secret, prv_key_data_id, &account_args).await?;

        Ok(())
    }
}

#[async_trait]
impl Cli for WalletCli {
    fn init(&self, term: &Arc<Terminal>) -> TerminalResult<()> {
        *self.term.lock().unwrap() = Some(term.clone());
        Ok(())
    }

    async fn digest(&self, term: Arc<Terminal>, cmd: String) -> TerminalResult<()> {
        let argv = parse(&cmd);
        let action: Action = argv[0].as_str().try_into()?;
        self.action(action, argv, term).await?;
        Ok(())
    }

    async fn complete(&self, _term: Arc<Terminal>, _cmd: String) -> TerminalResult<Vec<String>> {
        // TODO
        // let argv = parse(&cmd);
        Ok(vec![])
        // if argv.len() == 1 {
        //     // let part = argv.first().unwrap().as_str();
        //     // let mut list = vec![];
        //     // for (cmd,_) in HELP.iter() {
        //     //     if cmd.starts_with(part) {
        //     //         list.push(cmd.to_string());
        //     //     }
        //     // };
        //     // Ok(list)
        //     Ok(vec![])
        // } else {
        //     Ok(vec![])
        // }
    }
}

impl WalletCli {}

use kaspa_wallet_core::runtime::{self, BalanceStrings, PrvKeyDataCreateArgs};
async fn select_account(term: &Arc<Terminal>, wallet: &Arc<Wallet>) -> Result<Arc<runtime::Account>> {
    let mut selection = None;

    let mut list_by_key = Vec::<(Arc<PrvKeyDataInfo>, Vec<(usize, Arc<runtime::Account>)>)>::new();
    let mut flat_list = Vec::<Arc<runtime::Account>>::new();

    let mut keys = wallet.keys().await?;
    while let Some(key) = keys.try_next().await? {
        let mut prv_key_accounts = Vec::new();
        let mut accounts = wallet.accounts(Some(key.id)).await?;
        while let Some(account) = accounts.next().await {
            let account = account?;
            prv_key_accounts.push((flat_list.len(), account.clone()));
            flat_list.push(account.clone());
        }

        list_by_key.push((key.clone(), prv_key_accounts));
    }

    while selection.is_none() {
        list_by_key.iter().for_each(|(prv_key_data_info, accounts)| {
            term.writeln(format!("{prv_key_data_info}"));

            accounts.iter().for_each(|(seq, account)| {
                term.writeln(format!("    {seq}: {}", account.get_ls_string().unwrap_or_else(|err| panic!("{err}"))));
            })
        });

        let text = term
            .ask(false, &format!("Please select account ({}..{}) or <enter> to abort: ", 0, flat_list.len() - 1))
            .await?
            .trim()
            .to_string();
        if text.is_empty() {
            term.writeln("aborting...");
            return Err(Error::UserAbort);
        } else {
            // if let Ok(v) = serde_json::from_str::<T>(text.as_str()) {
            //     selection = Some(v);
            // } else {
            match text.parse::<usize>() {
                Ok(seq) if seq > 0 && seq < flat_list.len() => selection = flat_list.get(seq).cloned(),
                _ => {}
            };
            // }
        }
    }

    Ok(selection.unwrap())
}

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
            term.writeln(format!("{}: {} ({})", seq + 1, item, item.id().to_hex()));
        });

        let text = term.ask(false, &format!("{prompt} ({}..{}) or <enter> to abort: ", 0, list.len() - 1)).await?.trim().to_string();
        if text.is_empty() {
            term.writeln("aborting...");
            return Err(Error::UserAbort);
        } else {
            // if let Ok(v) = serde_json::from_str::<T>(text.as_str()) {
            //     selection = Some(v);
            // } else {
            match text.parse::<usize>() {
                Ok(seq) if seq > 0 && seq < list.len() => selection = list.get(seq).cloned(),
                _ => {}
            };
            // }
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

pub async fn kaspa_wallet_cli(options: TerminalOptions) -> Result<()> {
    cfg_if! {
        if #[cfg(not(target_arch = "wasm32"))] {
            kaspa_core::panic::configure_panic();
            kaspa_core::log::init_logger(None, "info");
        }
    }
    

    let wallet = Arc::new(Wallet::try_new(Wallet::local_store()?, None)?);
    let cli = Arc::new(WalletCli::new(wallet.clone()));
    let term = Arc::new(Terminal::try_new_with_options(cli.clone(), options)?);
    term.init().await?;

    // redirect the global log output to terminal
    #[cfg(not(target_arch = "wasm32"))]
    workflow_log::pipe(Some(cli.clone()));

    // cli starts notification->term trace pipe task
    cli.start().await?;
    term.writeln(format!("Kaspa Cli Wallet v{} (type 'help' for list of commands)", env!("CARGO_PKG_VERSION")));
    // wallet starts rpc and notifier
    wallet.start().await?;
    // terminal blocks async execution, delivering commands to the terminals
    term.run().await?;

    // wallet stops the notifier
    wallet.stop().await?;
    // cli stops notification->term trace pipe task
    cli.stop().await?;
    Ok(())
}
