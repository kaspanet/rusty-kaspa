use crate::actions::*;
use crate::helpers;
use crate::result::Result;
use async_trait::async_trait;
use futures::*;
use kaspa_wallet_core::accounts::gen0::import::*;
use kaspa_wallet_core::storage::AccountKind;
use kaspa_wallet_core::{runtime::wallet::AccountCreateArgs, runtime::Wallet, secret::Secret};
use kaspa_wallet_core::{Address, AddressPrefix};
use std::sync::{Arc, Mutex};
use workflow_core::abortable::Abortable;
use workflow_core::channel::*;
use workflow_core::runtime;
use workflow_log::*;
use workflow_terminal::Terminal;
pub use workflow_terminal::{parse, Cli, Options as TerminalOptions, Result as TerminalResult, TargetElement as TerminalTarget};

struct WalletCli {
    term: Arc<Mutex<Option<Arc<Terminal>>>>,
    wallet: Arc<Wallet>,
    notifications_task_ctl: DuplexChannel,
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
        WalletCli { term: Arc::new(Mutex::new(None)), wallet, notifications_task_ctl: DuplexChannel::oneshot() }
    }

    fn term(&self) -> Option<Arc<Terminal>> {
        self.term.lock().unwrap().as_ref().cloned()
    }

    async fn action(&self, action: Action, mut argv: Vec<String>, term: Arc<Terminal>) -> Result<()> {
        argv.remove(0);

        match action {
            Action::Help => {
                term.writeln("\n\rCommands:\n\r");
                display_help(&term);
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
                    // - TODO use Store to load settings
                } else if argv.len() != 2 {
                    term.writeln("\n\rError:\n\r");
                    term.writeln("Usage:\n\rset <key> <value>");
                    return Ok(());
                }
            }
            Action::Connect => {
                self.wallet.rpc_client().connect(true).await?;
            }
            Action::Disconnect => {
                self.wallet.rpc_client().shutdown().await?;
            }
            Action::GetInfo => {
                let response = self.wallet.get_info().await?;
                term.writeln(response);
            }
            Action::Ping => {
                self.wallet.ping().await?;
                term.writeln("ok");
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
                use kaspa_wallet_core::error::Error;

                let title = term.ask(false, "Wallet title: ").await?.trim().to_string();
                let wallet_password = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                let payment_password = Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec());
                let account_kind = AccountKind::Bip32;
                let mut args = AccountCreateArgs::new(title, account_kind, wallet_password, Some(payment_password));
                let res = self.wallet.create_wallet(&args).await;
                let (path, mnemonic) = if let Err(err) = res {
                    if !matches!(err, Error::WalletAlreadyExists) {
                        return Err(err.into());
                    }
                    let override_it = term
                        .ask(false, "Wallet already exists. Are you sure you want to override it (type 'y' to approve)?: ")
                        .await?;
                    let override_it = override_it.trim().to_string();
                    if !override_it.eq("y") {
                        return Ok(());
                    }
                    args.override_wallet = true;
                    self.wallet.create_wallet(&args).await?
                } else {
                    res.ok().unwrap()
                };

                // let mnemonic_phrase = String::from_utf8_lossy(secret.as_ref());
                term.writeln(format!("Default account mnemonic:\n{}\n", mnemonic.phrase()));
                term.writeln(format!("Wrote the wallet into '{}'\n", path.to_str().unwrap()));

                // - TODO - Select created as a current account
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

                if self.wallet.is_account_key_encrypted(&account, password.clone()).await? {
                    _payment_secret = Some(Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()));
                }
                let keydata = self.wallet.get_account_keydata(account.prv_key_data_id, password.clone()).await?;
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

                let priority_fee_sompi = if let Some(fee) = priority_fee { helpers::kas_str_to_sompi(fee)? } else { 0u64 };
                let address = serde_json::from_str::<Address>(address)?;
                let amount_sompi = helpers::kas_str_to_sompi(amount)?;

                let password = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                let mut payment_secret = Option::<Secret>::None;

                if self.wallet.is_account_key_encrypted(&account, password.clone()).await? {
                    payment_secret = Some(Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()));
                }
                let keydata = self.wallet.get_account_keydata(account.prv_key_data_id, password.clone()).await?;
                if keydata.is_none() {
                    return Err("It is read only wallet.".into());
                }
                let abortable = Abortable::default();
                let ids =
                    account.send(&address, amount_sompi, priority_fee_sompi, keydata.unwrap(), payment_secret, &abortable).await?;

                term.writeln(format!("\r\nSending {amount} KAS to {address}, tx ids:"));
                term.writeln(format!("{}\r\n", ids.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\r\n")));
            }
            Action::Address => {
                let address = self.wallet.account()?.address().await?.to_string();
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
                    "kaspanet" => {
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
                        } else if runtime::is_web() {
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
                let map = self.wallet.account_map().locked_map();
                for (prv_key_data_id, list) in map.iter() {
                    term.writeln(format!("key: {}", prv_key_data_id.to_hex()));
                    for account in list.iter() {
                        term.writeln(account.get_ls_string());
                    }
                }
            }
            Action::Select => {
                if argv.is_empty() {
                    self.wallet.select(None).await?;
                } else {
                    let name = argv.remove(0);
                    let account = {
                        let accounts = self.wallet.account_list()?;
                        accounts.iter().position(|account| account.name() == name).map(|index| accounts.get(index).unwrap().clone())
                    };
                    self.wallet.select(account).await?;
                }
            }
            Action::Open => {
                let mut prefix = AddressPrefix::Mainnet;
                if argv.contains(&"testnet".to_string()) {
                    prefix = AddressPrefix::Testnet;
                } else if argv.contains(&"simnet".to_string()) {
                    prefix = AddressPrefix::Simnet;
                } else if argv.contains(&"devnet".to_string()) {
                    prefix = AddressPrefix::Devnet;
                }
                let secret = Secret::new(term.ask(true, "Enter wallet password:").await?.trim().as_bytes().to_vec());
                self.wallet.load(secret, prefix).await?;
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
        // log_info!("### starting notification processor");
        let self_ = self.clone();
        let term = self.term().unwrap_or_else(|| panic!("WalletCli::notification_pipe_task(): `term` is not initialized"));
        let notification_channel_receiver = self.wallet.rpc_client().notification_channel_receiver();
        workflow_core::task::spawn(async move {
            // term.writeln(args.to_string());
            loop {
                select! {

                    _ = self_.notifications_task_ctl.request.receiver.recv().fuse() => {
                        // if let Ok(msg) = msg {
                        //     let text = format!("{msg:#?}").replace('\n',"\r\n");
                        //     println!("#### text: {text:?}");
                        //     term.pipe_crlf.send(text).await.unwrap_or_else(|err|log_error!("WalletCli::notification_pipe_task() unable to route to term: `{err}`"));
                        // }
                        break;
                    },
                    msg = notification_channel_receiver.recv().fuse() => {
                        if let Ok(msg) = msg {
                            let text = format!("{msg:#?}").replace('\n',"\r\n"); //.payload);
                            term.pipe_crlf.send(text).await.unwrap_or_else(|err|log_error!("WalletCli::notification_pipe_task() unable to route to term: `{err}`"));
                        }
                    }
                }
            }

            self_
                .notifications_task_ctl
                .response
                .sender
                .send(())
                .await
                .unwrap_or_else(|err| log_error!("WalletCli::notification_pipe_task() unable to signal task shutdown: `{err}`"));
        });
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

pub async fn kaspa_wallet_cli(options: TerminalOptions) -> Result<()> {
    let wallet = Arc::new(Wallet::try_new().await?);
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
