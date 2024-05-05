use crate::imports::*;
use workflow_core::channel::*;
use workflow_terminal::clear::*;
use workflow_terminal::cursor::*;

pub struct Monitor {
    shutdown_tx: Arc<Mutex<Option<Sender<()>>>>,
}

impl Default for Monitor {
    fn default() -> Self {
        Monitor { shutdown_tx: Arc::new(Mutex::new(None)) }
    }
}

#[async_trait]
impl Handler for Monitor {
    fn verb(&self, _ctx: &Arc<dyn Context>) -> Option<&'static str> {
        Some("monitor")
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Balance monitor"
    }

    async fn stop(self: Arc<Self>, _ctx: &Arc<dyn Context>) -> cli::Result<()> {
        let shutdown_tx = self.shutdown_tx.lock().unwrap().take();
        if let Some(shutdown_tx) = shutdown_tx {
            shutdown_tx.send(()).await.ok();
        }
        Ok(())
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(&ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Monitor {
    async fn main(self: Arc<Self>, ctx: &Arc<KaspaCli>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let max_events = 16;
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let events_rx = ctx.wallet().multiplexer().channel();

        let (shutdown_tx, shutdown_rx) = oneshot();
        self.shutdown_tx.lock().unwrap().replace(shutdown_tx.clone());
        let mut interval = interval(Duration::from_millis(1000));

        let term = ctx.term();
        spawn(async move {
            term.kbhit(None).await.ok();
            shutdown_tx.send(()).await.ok();
        });

        let ctx = ctx.clone();
        let this = self.clone();
        spawn(async move {
            loop {
                select! {

                    event = events_rx.recv().fuse() => {
                        if let Ok(event) = event {
                            let mut events = events.lock().unwrap();
                            events.push_front(event);
                            while events.len() > max_events {
                                events.pop_back();
                            }
                        }
                    }

                    _ = interval.next().fuse() => {
                        this.redraw(&ctx, &events).await.ok();
                        yield_executor().await;
                    }

                    _ = shutdown_rx.recv().fuse() => {
                        break;
                    }

                }
            }

            tprint!(ctx, "{}", ClearScreen);
            tprint!(ctx, "{}", Goto(1, 1));
            this.shutdown_tx.lock().unwrap().take();
            ctx.term().refresh_prompt();
        });

        Ok(())
    }

    async fn redraw(self: &Arc<Self>, ctx: &Arc<KaspaCli>, events: &Arc<Mutex<VecDeque<Box<Events>>>>) -> Result<()> {
        tprint!(ctx, "{}", ClearScreen);
        tprint!(ctx, "{}", Goto(1, 1));

        let wallet = ctx.wallet();

        if !wallet.is_connected() {
            tprintln!(ctx, "{}", style("Wallet is not connected to the network").magenta());
            tprintln!(ctx);
        } else if !wallet.is_synced() {
            tprintln!(ctx, "{}", style("Kaspa node is currently syncing").magenta());
            tprintln!(ctx);
        }

        ctx.list().await?;

        let events = events.lock().unwrap();
        events.iter().for_each(|event| match event.deref() {
            Events::DaaScoreChange { .. } => {}
            Events::Balance { balance, id } => {
                let network_id = wallet.network_id().expect("missing network type");
                let network_type = NetworkType::from(network_id);
                let balance_strings = BalanceStrings::from((balance.as_ref(), &network_type, None));
                let id = id.short();

                let mature_utxo_count =
                    balance.as_ref().map(|balance| balance.mature_utxo_count.separated_string()).unwrap_or("N/A".to_string());
                let pending_utxo_count = balance.as_ref().map(|balance| balance.pending_utxo_count).unwrap_or(0);

                let pending_utxo_info =
                    if pending_utxo_count > 0 { format!("({pending_utxo_count} pending)") } else { "".to_string() };
                let utxo_info = style(format!("{mature_utxo_count} UTXOs {pending_utxo_info}")).dim();

                tprintln!(ctx, "{} {id}: {balance_strings}   {utxo_info}", style("balance".pad_to_width(8)).blue());
            }
            _ => {}
        });

        Ok(())
    }
}
