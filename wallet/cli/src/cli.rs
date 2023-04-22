use crate::actions::*;
use crate::helpers::*;
use crate::result::Result;
use async_trait::async_trait;
use futures::*;
use kaspa_wallet_core::storage::AccountKind;
use kaspa_wallet_core::{secret::Secret, wallet::AccountCreateArgs, Wallet};
use std::sync::{Arc, Mutex};
use workflow_core::channel::*;
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
        self.term.lock().unwrap().as_ref().cloned() //map(|term| term.clone())
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
                self.wallet.rpc.connect(true).await?;
            }
            Action::Disconnect => {
                self.wallet.rpc.shutdown().await?;
            }
            Action::GetInfo => {
                let response = self.wallet.get_info().await?;
                term.writeln(response);
            }
            Action::Ping => {
                self.wallet.ping().await?;
                term.writeln("ok");
            }
            Action::Balance => {
                self.wallet.balance().await?;
            }
            Action::Create => {
                use kaspa_wallet_core::error::Error;

                let title = term.ask(false, "Wallet title: ").await?.trim().to_string();
                let wallet_password = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                let payment_password = Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec());
                let account_kind = AccountKind::Bip32;
                let mut args = AccountCreateArgs::new(title, account_kind, wallet_password.clone(), Some(payment_password.clone()));
                let res = self.wallet.create(&args).await;
                let path = if let Err(err) = res {
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
                    self.wallet.create(&args).await?
                } else {
                    res.ok().unwrap()
                };

                term.writeln(format!("Wrote the wallet into '{}'\n", path.to_str().unwrap()));
            }
            Action::Broadcast => {
                self.wallet.broadcast().await?;
            }
            Action::CreateUnsignedTx => {
                self.wallet.create_unsigned_transaction().await?;
            }
            Action::DumpUnencrypted => {
                self.wallet.dump_unencrypted().await?;
            }
            Action::NewAddress => {
                let response = self.wallet.new_address().await?;
                term.writeln(response);
            }
            Action::Parse => {
                self.wallet.parse().await?;
            }
            Action::Send => {
                self.wallet.send().await?;
            }
            Action::ShowAddress => {
                self.wallet.show_address().await?;
            }
            Action::Sign => {
                self.wallet.sign().await?;
            }
            Action::Sweep => {
                self.wallet.sweep().await?;
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
                        let mnemonic = ask_mnemonic(&term).await?;
                        log_info!("Mnemonic: {:?}", mnemonic);
                    }
                    "kaspanet" => {
                        // Wallet::import_kaspanet(&term).await?;
                        // load_v0_keydata()
                    }
                    "kaspa-wallet" => {}
                    _ => {
                        return Err(format!("Invalid argument: {}", what).into());
                    }
                }
            }

            // ~~~
            Action::List => {
                let accounts = self.wallet.accounts().await;
                for account in accounts.iter() {
                    term.writeln(account.get_ls_string());
                }
            }
            Action::Select => {
                if argv.is_empty() {
                    self.wallet.select(None).await?;
                } else {
                    let name = argv.remove(0);
                    let accounts = self.wallet.accounts().await;
                    let account =
                        accounts.iter().position(|account| account.name() == name).map(|index| accounts.get(index).unwrap().clone());
                    self.wallet.select(account).await?;
                    // if let Some(idx) = accounts.iter().position(|account| account.name() == name) {
                    //     self.wallet.select(Some(accounts.get(idx).unwrap().clone())).await?;
                    // } else {
                    //     self.wallet.select(None).await?;
                    // }
                }

                // TODO
            }
            Action::Open => {
                let secret = Secret::new(term.ask(true, "Enter wallet password:").await?.trim().as_bytes().to_vec());
                self.wallet.load_accounts(secret).await?;
            }
            Action::Close => {
                self.wallet.clear().await?;
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
        let notification_channel_receiver = self.wallet.rpc.notification_channel_receiver();
        workflow_core::task::spawn(async move {
            // term.writeln(args.to_string());
            loop {
                select! {

                    _ = self_.notifications_task_ctl.request.receiver.recv().fuse() => {
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
