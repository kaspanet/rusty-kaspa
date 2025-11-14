use async_trait::async_trait;
use clap::Parser;
use futures::FutureExt;
use kaspa_consensus::{
    consensus::storage::ConsensusStorage,
    model::stores::{acceptance_data::AcceptanceDataStoreReader, block_transactions::BlockTransactionsStoreReader},
};
use kaspa_consensus_core::{
    config::ConfigBuilder,
    network::{NetworkId, NetworkType},
    Hash,
};
use kaspa_database::prelude::{ConnBuilder, DB};
use log::warn;
use std::{
    error::Error,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{select, time::sleep};
use workflow_core::channel::DuplexChannel;
use workflow_terminal::{Cli, Result as TerminalResult, Terminal};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    db_path: String,
}

struct Playground {
    term: Arc<Mutex<Option<Arc<Terminal>>>>,
    db: Arc<DB>,
    storage: Arc<ConsensusStorage>,
    task_ctl: DuplexChannel,
    shutdown: Arc<AtomicBool>,
}

impl Playground {
    fn try_new(db_path: &str) -> Result<Self, Box<dyn Error>> {
        let network = NetworkId::new(NetworkType::Mainnet);
        let config = Arc::new(ConfigBuilder::new(network.into()).adjust_perf_params_to_consensus_params().build());
        let db = ConnBuilder::default()
            .with_db_path(Path::new(db_path).to_path_buf())
            .with_files_limit(128)
            .build_secondary(Path::new(db_path).with_extension("secondary"))?;

        let storage = ConsensusStorage::new(db.clone(), config.clone());

        Ok(Self {
            term: Arc::new(Mutex::new(None)),
            db,
            storage,
            task_ctl: DuplexChannel::oneshot(),
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    fn term(&self) -> Arc<Terminal> {
        self.term.lock().unwrap().as_ref().cloned().expect("Terminal not initialized")
    }

    fn start(self: &Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                select! {
                    _ = this.task_ctl.request.receiver.recv().fuse() => {
                        break;
                    },
                    _ = sleep(Duration::from_millis(1000)).fuse() => {
                        if let Err(e) = this.update_and_draw().await {
                            warn!("Error in monitoring task: {}", e);
                        }
                    }
                }
            }
            this.task_ctl
                .response
                .sender
                .send(())
                .await
                .unwrap_or_else(|err| warn!("playground: unable to signal task shutdown: `{err}`"));
        });
    }

    async fn update_and_draw(&self) -> Result<(), Box<dyn Error>> {
        let term = self.term();

        if self.db.try_catch_up_with_primary().is_err() {
            term.writeln("Syncing with primary DB...");
            sleep(Duration::from_millis(200)).await;
            return Ok(());
        }

        Ok(())
    }

    fn get_accepting_block_info(&self, accepting_block_hash: Hash) -> Result<(), kaspa_database::prelude::StoreError> {
        self.db.try_catch_up_with_primary()?;

        let term = self.term();
        term.writeln(format!("Accepting block: {}", accepting_block_hash));

        let acceptance_data = self.storage.acceptance_data_store.get(accepting_block_hash)?;
        let mergeset_block_hashes: Vec<Hash> = acceptance_data.iter().map(|ms_block_data| ms_block_data.block_hash).collect();
        term.writeln("Mergeset blocks:".to_string());
        for hash in &mergeset_block_hashes {
            term.writeln(format!("  {}", hash));
        }

        let accepting_block_transactions = self.storage.block_transactions_store.get(accepting_block_hash)?;
        term.writeln(format!("Transactions in accepting block {}:", accepting_block_hash));
        for tx in accepting_block_transactions.iter() {
            term.writeln(format!("  {}", tx.id()));
        }

        term.writeln("Accepted transactions from mergeset:".to_string());

        for mergeset_block_data in acceptance_data.iter() {
            term.writeln(format!("  From block {}:", mergeset_block_data.block_hash));
            if mergeset_block_data.accepted_transactions.is_empty() {
                term.writeln("    (none)".to_string());
            }
            for accepted_tx in &mergeset_block_data.accepted_transactions {
                term.writeln(format!("    {}", accepted_tx.transaction_id));
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Cli for Playground {
    fn init(self: Arc<Self>, term: &Arc<Terminal>) -> TerminalResult<()> {
        *self.term.lock().unwrap() = Some(term.clone());
        self.start();
        Ok(())
    }

    async fn digest(self: Arc<Self>, term: Arc<Terminal>, cmd: String) -> TerminalResult<()> {
        let cmd = cmd.trim();
        if cmd == "quit" || cmd == "exit" {
            self.shutdown.store(true, Ordering::SeqCst);
            self.task_ctl.signal(()).await?;
            term.exit().await;
        } else if cmd.starts_with("get_accepting_block ") {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if parts.len() == 2 {
                match parts[1].parse::<Hash>() {
                    Ok(hash) => {
                        if let Err(e) = self.get_accepting_block_info(hash) {
                            term.writeln(format!("Error: {}", e));
                        }
                    }
                    Err(e) => {
                        term.writeln(format!("Invalid hash: {}", e));
                    }
                }
            } else {
                term.writeln("Usage: get_accepting_block <hash>");
            }
        } else if !cmd.is_empty() {
            term.writeln("Unknown command. Type 'quit' or 'exit' to close.");
        }
        Ok(())
    }

    async fn complete(self: Arc<Self>, _term: Arc<Terminal>, _cmd: String) -> TerminalResult<Option<Vec<String>>> {
        Ok(None)
    }

    fn prompt(&self) -> Option<String> {
        if self.shutdown.load(Ordering::SeqCst) {
            None
        } else {
            Some("> ".to_string())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    kaspa_core::log::init_logger(None, "");

    let args = Args::parse();

    let playground = Arc::new(Playground::try_new(&args.db_path)?);

    let term = Arc::new(Terminal::try_new_with_options(playground.clone(), workflow_terminal::Options::default())?);

    term.init().await?;

    term.writeln("Starting playground...".to_string());
    term.writeln(format!("Using database at: {}", args.db_path));
    term.writeln("Type 'quit' or 'exit' to stop.");
    sleep(Duration::from_secs(2)).await;

    term.run().await?;

    Ok(())
}
