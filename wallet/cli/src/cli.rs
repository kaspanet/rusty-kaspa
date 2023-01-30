use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use workflow_terminal::Terminal;
// use workflow_terminal::Options;
use crate::actions::*;
use crate::result::Result;
use futures::*;
use kaspa_wallet_core::Wallet;
use workflow_core::channel::*;
use workflow_log::*;
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

    async fn action(&self, action: Action, argv: Vec<String>, term: Arc<Terminal>) -> Result<()> {
        match action {
            Action::Help => {
                term.writeln("\n\rCommands:\n\r");
                display_help(&term);
            }
            Action::Exit => {
                term.writeln("bye!");
                term.exit().await;
            }
            Action::GetInfo => {
                //log_trace!("testing 123");
                // let msg = argv[1..].join(" ");
                let response = self.wallet.info().await?;
                term.writeln(response);
            }
            Action::Ping => {
                let msg = argv[1..].join(" ");
                let response = self.wallet.ping(msg).await?;
                term.writeln(response);
            }
            Action::Balance => {
                self.wallet.balance().await?;
            }
            Action::Create => {
                self.wallet.create().await?;
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
                // let listener =
                self.wallet.subscribe_daa_score().await?;
                // workflow_core::task::spawn(async move {
                //     let term = term;
                //     let channel = listener.recv_channel;
                //     while let Ok(notification) = channel.recv().await {
                //         log_trace!("DAA notification: {:?}", notification);
                //         //sender.send(notification)
                //         term.writeln(format!("{notification:#?}").replace("\n", "\n\r"));
                //     }
                // });
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
        let self_ = self.clone();
        let term = self.term().unwrap_or_else(|| panic!("WalletCli::notification_pipe_task(): `term` is not initialized"));
        let notification_channel_receiver = self.wallet.notification_channel_receiver();
        workflow_core::task::spawn(async move {
            // term.writeln(args.to_string());
            loop {
                select! {

                    _ = self_.notifications_task_ctl.request.receiver.recv().fuse() => {
                        break;
                    },
                    msg = notification_channel_receiver.recv().fuse() => {
                        if let Ok(msg) = msg {
                            let text = format!("{:#?}",msg.payload);
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

    #[cfg(not(target_arch = "wasm32"))]
    workflow_log::pipe(Some(cli.clone()));

    cli.start().await?;

    term.writeln("Kaspa Cli Wallet (type 'help' for list of commands)");
    wallet.start().await?;
    term.run().await?;
    wallet.stop().await?;
    cli.stop().await?;
    Ok(())
}
