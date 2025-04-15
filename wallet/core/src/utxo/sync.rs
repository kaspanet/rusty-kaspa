//!
//! Sync monitor implementation. Sync monitor tracks
//! the node's sync state and notifies the wallet.
//!

use crate::imports::*;
use crate::result::Result;
use futures::pin_mut;
use futures::stream::StreamExt;
use regex::Regex;
struct Inner {
    task_ctl: DuplexChannel,
    rpc: Mutex<Option<Rpc>>,
    multiplexer: Multiplexer<Box<Events>>,
    running: AtomicBool,
    is_synced: AtomicBool,
    state_observer: StateObserver,
}

#[derive(Clone)]
pub struct SyncMonitor {
    inner: Arc<Inner>,
}

impl SyncMonitor {
    pub fn new(rpc: Option<Rpc>, multiplexer: &Multiplexer<Box<Events>>) -> Self {
        Self {
            inner: Arc::new(Inner {
                rpc: Mutex::new(rpc.clone()),
                multiplexer: multiplexer.clone(),
                task_ctl: DuplexChannel::oneshot(),
                running: AtomicBool::new(false),
                is_synced: AtomicBool::new(false),
                state_observer: StateObserver::default(),
            }),
        }
    }

    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::SeqCst)
    }

    pub fn is_synced(&self) -> bool {
        self.inner.is_synced.load(Ordering::SeqCst)
    }

    pub async fn track(&self, is_synced: bool) -> Result<()> {
        if self.is_synced() != is_synced || !is_synced && !self.is_running() {
            if is_synced {
                // log_trace!("sync monitor: node synced state detected");
                self.inner.is_synced.store(true, Ordering::SeqCst);
                if self.is_running() {
                    log_trace!("sync monitor: stopping sync monitor task");
                    self.stop_task().await?;
                }
                self.notify(Events::SyncState { sync_state: SyncState::Synced }).await?;
            } else {
                self.inner.is_synced.store(false, Ordering::SeqCst);
                // log_trace!("sync monitor: node is not synced");
                if !self.is_running() {
                    log_trace!("sync monitor: starting sync monitor task");
                    self.start_task().await?;
                }
                self.notify(Events::SyncState { sync_state: SyncState::NotSynced }).await?;
            }
        }

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.inner.is_synced.store(false, Ordering::SeqCst);
        if self.is_running() {
            self.stop_task().await?;
        }
        Ok(())
    }

    pub fn rpc_api(&self) -> Arc<DynRpcApi> {
        self.inner.rpc.lock().unwrap().as_ref().expect("SyncMonitor RPC not initialized").rpc_api().clone()
    }

    pub async fn bind_rpc(&self, rpc: Option<Rpc>) -> Result<()> {
        *self.inner.rpc.lock().unwrap() = rpc;
        Ok(())
    }

    pub fn multiplexer(&self) -> &Multiplexer<Box<Events>> {
        &self.inner.multiplexer
    }

    pub async fn notify(&self, event: Events) -> Result<()> {
        self.multiplexer()
            .try_broadcast(Box::new(event))
            .map_err(|_| Error::Custom("multiplexer channel error during update_balance".to_string()))?;
        Ok(())
    }

    async fn handle_event(&self, event: Box<Events>) -> Result<()> {
        match *event {
            Events::UtxoProcStart => {}
            Events::UtxoProcStop => {}
            _ => {}
        }

        Ok(())
    }

    async fn get_sync_status(&self) -> Result<bool> {
        Ok(self.rpc_api().get_sync_status().await?)
    }

    pub async fn start_task(&self) -> Result<()> {
        if self.is_running() {
            panic!("SyncProc::start_task() called while already running");
        }

        let this = self.clone();
        this.inner.running.store(true, Ordering::SeqCst);
        let task_ctl_receiver = self.inner.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.inner.task_ctl.response.sender.clone();
        let events = self.multiplexer().channel();

        spawn(async move {
            let interval = interval(Duration::from_secs(5));
            pin_mut!(interval);

            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },

                    _ = interval.next().fuse() => {
                        if this.is_synced() {
                            break;
                        } else if let Ok(is_synced) = this.get_sync_status().await {
                            if is_synced {
                                if is_synced != this.is_synced() {
                                    this.inner.is_synced.store(true, Ordering::SeqCst);
                                    this.notify(Events::SyncState { sync_state : SyncState::Synced }).await.unwrap_or_else(|err|log_error!("SyncProc error dispatching notification event: {err}"));
                                }

                                break;
                            }
                        }
                    }

                    msg = events.receiver.recv().fuse() => {
                        match msg {
                            Ok(event) => {
                                this.handle_event(event).await.unwrap_or_else(|e| log_error!("SyncProc::handle_event() error: {}", e));
                            },
                            Err(err) => {
                                log_error!("SyncProc: error while receiving multiplexer message: {err}");
                                log_error!("Suspending Wallet processing...");

                                break;
                            }
                        }
                    },
                }
            }

            log_trace!("sync monitor task is shutting down...");
            this.inner.running.store(false, Ordering::SeqCst);
            task_ctl_sender.send(()).await.unwrap();
        });
        Ok(())
    }

    pub async fn stop_task(&self) -> Result<()> {
        self.inner.task_ctl.signal(()).await.expect("SyncProc::stop_task() `signal` error");
        Ok(())
    }

    pub async fn handle_stdout(&self, text: &str) -> Result<()> {
        let lines = text.split('\n').collect::<Vec<_>>();

        let mut state: Option<SyncState> = None;
        for line in lines {
            if !line.is_empty() {
                if let Some(new_state) = self.inner.state_observer.get(line) {
                    state.replace(new_state);
                }
            }
        }
        if let Some(sync_state) = state {
            self.notify(Events::SyncState { sync_state }).await?;
        }

        Ok(())
    }
}

// This is a temporary implementation that extracts sync state from the node's stdout.
// This will be removed one a proper RPC notification API is implemented.
pub struct StateObserver {
    proof: Regex,
    ibd_headers: Regex,
    ibd_blocks: Regex,
    utxo_resync: Regex,
    utxo_sync: Regex,
    trust_blocks: Regex,
    // accepted_block: Regex,
}

impl Default for StateObserver {
    fn default() -> Self {
        Self {
            proof: Regex::new(r"Validating level (\d+) from the pruning point proof").unwrap(),
            ibd_headers: Regex::new(r"IBD: Processed (\d+) block headers \((\d+)%\)").unwrap(),
            ibd_blocks: Regex::new(r"IBD: Processed (\d+) blocks \((\d+)%\)").unwrap(),
            utxo_resync: Regex::new(r"Resyncing the utxoindex...").unwrap(),
            utxo_sync: Regex::new(r"Received (\d+) UTXO set chunks so far, totaling in (\d+) UTXOs").unwrap(),
            trust_blocks: Regex::new(r"Processed (\d) trusted blocks in the last .* (total (\d))").unwrap(),
            // accepted_block: Regex::new(r"Accepted block .* via").unwrap(),
        }
    }
}

impl StateObserver {
    pub fn get(&self, line: &str) -> Option<SyncState> {
        let mut state: Option<SyncState> = None;

        if let Some(captures) = self.ibd_headers.captures(line) {
            if let (Some(headers), Some(progress)) = (captures.get(1), captures.get(2)) {
                if let (Ok(headers), Ok(progress)) = (headers.as_str().parse::<u64>(), progress.as_str().parse::<u64>()) {
                    state = Some(SyncState::Headers { headers, progress });
                }
            }
        } else if let Some(captures) = self.ibd_blocks.captures(line) {
            if let (Some(blocks), Some(progress)) = (captures.get(1), captures.get(2)) {
                if let (Ok(blocks), Ok(progress)) = (blocks.as_str().parse::<u64>(), progress.as_str().parse::<u64>()) {
                    state = Some(SyncState::Blocks { blocks, progress });
                }
            }
        } else if let Some(captures) = self.utxo_sync.captures(line) {
            if let (Some(chunks), Some(total)) = (captures.get(1), captures.get(2)) {
                if let (Ok(chunks), Ok(total)) = (chunks.as_str().parse::<u64>(), total.as_str().parse::<u64>()) {
                    state = Some(SyncState::UtxoSync { chunks, total });
                }
            }
        } else if let Some(captures) = self.trust_blocks.captures(line) {
            if let (Some(processed), Some(total)) = (captures.get(1), captures.get(2)) {
                if let (Ok(processed), Ok(total)) = (processed.as_str().parse::<u64>(), total.as_str().parse::<u64>()) {
                    state = Some(SyncState::TrustSync { processed, total });
                }
            }
        } else if let Some(captures) = self.proof.captures(line) {
            if let Some(level) = captures.get(1) {
                if let Ok(level) = level.as_str().parse::<u64>() {
                    state = Some(SyncState::Proof { level });
                }
            }
        } else if self.utxo_resync.is_match(line) {
            state = Some(SyncState::UtxoResync);
        }

        state
    }
}
