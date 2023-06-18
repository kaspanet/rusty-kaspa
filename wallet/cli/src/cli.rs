use crate::actions::*;
use crate::error::Error;
use crate::helpers;
use crate::result::Result;
use async_trait::async_trait;
use futures::*;
use kaspa_wallet_core::accounts::gen0::import::*;
use kaspa_wallet_core::imports::ToHex;
use kaspa_wallet_core::iterator::IteratorOptions;
use kaspa_wallet_core::runtime::wallet::WalletCreateArgs;
use kaspa_wallet_core::storage::{AccountKind, IdT, PrvKeyDataId};
use kaspa_wallet_core::{runtime::wallet::AccountCreateArgs, runtime::Wallet, secret::Secret};
use kaspa_wallet_core::{Address, AddressPrefix};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use textwrap::wrap;
use workflow_core::abortable::Abortable;
use workflow_core::channel::*;
use workflow_core::enums::EnumTrait;
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
                let is_open = self.wallet.is_open().await?;

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

                        let account_kind: AccountKind = select_variant(&term, "Please select account type", &mut argv).await?;

                        let prv_key_data_info = select_item(
                            &term,
                            "Please select private key",
                            &mut argv,
                            self.wallet.store().as_prv_key_data_store()?.iter(IteratorOptions::default()).await?,
                        )
                        .await?;

                        self.create_account(prv_key_data_info.id, account_kind, term).await?;
                    }
                    _ => {
                        term.writeln("\n\rError:\n\r");
                        term.writeln("Usage:\n\rcreate <account|wallet>");
                        return Ok(());
                    }
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

                let priority_fee_sompi = if let Some(fee) = priority_fee { helpers::kas_str_to_sompi(fee)? } else { 0u64 };
                let address = serde_json::from_str::<Address>(address)?;
                let amount_sompi = helpers::kas_str_to_sompi(amount)?;

                let password = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                let mut payment_secret = Option::<Secret>::None;

                if self.wallet.is_account_key_encrypted(&account).await?.is_some_and(|f| f) {
                    payment_secret = Some(Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()));
                }
                let keydata = self.wallet.get_prv_key_data(password.clone(), &account.prv_key_data_id).await?;
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
                let mut keys = self.wallet.keys(IteratorOptions::default());
                while let Some(keys) = keys.next().await? {
                    for key in keys {
                        term.writeln(format!("key: {}", key.id.to_hex()));
                        let mut accounts = self.wallet.accounts(Some(key.id), IteratorOptions::default());
                        while let Some(accounts) = accounts.next().await? {
                            for account in accounts {
                                term.writeln(format!("account: {}", account.id.to_hex()));
                            }
                        }
                    }
                }

                // let map = self.wallet.account_map().locked_map();
                // for (prv_key_data_id, list) in map.iter() {
                //     term.writeln(format!("key: {}", prv_key_data_id.to_hex()));
                //     for account in list.iter() {
                //         term.writeln(account.get_ls_string());
                //     }
                // }
            }
            Action::Select => {
                if argv.is_empty() {
                    self.wallet.select(None).await?;
                } else {
                    let name = argv.remove(0);
                    let account = {
                        // let accounts = ;
                        self.wallet
                            .active_accounts()
                            .inner()
                            .values()
                            .find(|account| account.name() == name)
                            .ok_or(Error::AccountNotFound(name))?
                            .clone()
                    };
                    self.wallet.select(Some(account)).await?;
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

        let wallet_password = Secret::new(term.ask(true, "Enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
        if wallet_password.as_ref().is_empty() {
            return Err(Error::WalletSecretRequired.into());
        }
        let wallet_password_validate =
            Secret::new(term.ask(true, "Re-enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
        if wallet_password_validate.as_ref() != wallet_password.as_ref() {
            return Err(Error::WalletSecretMatch.into());
        }

        let payment_password = Secret::new(
            term.ask(true, "Enter payment (private key encryption) password (optional): ").await?.trim().as_bytes().to_vec(),
        );
        if !payment_password.as_ref().is_empty() {
            let payment_password_validate = Secret::new(
                term.ask(true, "Enter payment (private key encryption) password (optional): ").await?.trim().as_bytes().to_vec(),
            );
            if payment_password_validate.as_ref() != payment_password.as_ref() {
                return Err(Error::PaymentSecretMatch.into());
            }
        }

        let account_kind = AccountKind::Bip32;

        let wallet_args = WalletCreateArgs::new(None, hint, true);
        let account_args = AccountCreateArgs::new(account_name, account_title, account_kind, wallet_password, Some(payment_password));
        let mnemonic = self.wallet.create_wallet(&wallet_args, &account_args).await?;

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

        ["", "Never share your mnemonic with anyone!", "---", "", "Your default wallet account mnemonic:", mnemonic.phrase(), ""]
            .into_iter()
            .for_each(|line| term.writeln(line));

        Ok(())
    }

    async fn create_account(&self, _prv_key_data_id: PrvKeyDataId, account_kind: AccountKind, term: Arc<Terminal>) -> Result<()> {
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

#[allow(dead_code)]
// async fn select_item<T>(term : &Arc<Terminal>, prompt: &str, argv : &mut Vec<String>, mut iter : impl kaspa_wallet_core::iterator::Iterator<Item = T>) -> Result<T>
async fn select_item<T>(
    term: &Arc<Terminal>,
    prompt: &str,
    argv: &mut Vec<String>,
    mut iter: Box<dyn kaspa_wallet_core::iterator::Iterator<Item = Arc<T>>>,
) -> Result<Arc<T>>
where
    T: std::fmt::Display + IdT + Clone + Send + Sync + 'static,
{
    let mut selection = None;
    let list: Vec<Arc<T>> = iter.collect().await?;

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

async fn select_variant<T>(term: &Arc<Terminal>, prompt: &str, argv: &mut Vec<String>) -> Result<T>
where
    T: EnumTrait<T> + DeserializeOwned + Clone + Serialize,
{
    if !argv.is_empty() {
        let text = argv.remove(0);
        if let Ok(v) = serde_json::from_str::<T>(text.as_str()) {
            return Ok(v);
        } else {
            let accepted = T::list().iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join(", ");
            return Err(Error::UnrecognizedArgument(text, accepted));
        }
    }

    let mut selection = None;
    let list = T::list();
    while selection.is_none() {
        list.iter().enumerate().for_each(|(seq, item)| {
            let name = serde_json::to_string(item).unwrap();
            term.writeln(format!("{}: '{name}' - {}", seq, item.descr()));
        });

        let text = term.ask(false, &format!("{prompt} ({}..{}) or <enter> to abort: ", 0, list.len() - 1)).await?.trim().to_string();
        if text.is_empty() {
            term.writeln("aborting...");
            return Err(Error::UserAbort);
        } else if let Ok(v) = serde_json::from_str::<T>(text.as_str()) {
            selection = Some(v);
        } else {
            match text.parse::<usize>() {
                Ok(seq) if seq > 0 && seq < list.len() => selection = list.get(seq).cloned(),
                _ => {}
            };
        }
    }

    Ok(selection.unwrap())
}

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
