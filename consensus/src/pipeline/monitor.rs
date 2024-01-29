use super::ProcessingCounters;
use indicatif::ProgressBar;
use kaspa_core::{
    info,
    log::progressions::{maybe_init_spinner, MULTI_PROGRESS_BAR_ACTIVE},
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace,
};
use std::{
    borrow::Cow,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};

const MONITOR: &str = "consensus-monitor";
const SNAPSHOT_INTERVAL_IN_SECS: usize = 10;

pub struct ConsensusProgressBars {
    pub header_count: Option<ProgressBar>,
    pub block_count: Option<ProgressBar>,
    pub tx_count: Option<ProgressBar>,
    pub chain_block_count: Option<ProgressBar>,
    pub dep_count: Option<ProgressBar>,
    pub mergeset_count: Option<ProgressBar>,
    pub mass_count: Option<ProgressBar>,
}

impl ConsensusProgressBars {
    pub fn new() -> Option<Self> {
        if MULTI_PROGRESS_BAR_ACTIVE.load(Ordering::SeqCst) {
            return Some(Self {
                header_count: maybe_init_spinner(
                    Cow::Borrowed(ConsensusMonitor::IDENT),
                    Cow::Owned(format!("Header Count (last {}s)", SNAPSHOT_INTERVAL_IN_SECS).to_string()),
                ),
                block_count: maybe_init_spinner(
                    Cow::Borrowed(ConsensusMonitor::IDENT),
                    Cow::Owned(format!("Block Count (last {})", SNAPSHOT_INTERVAL_IN_SECS).to_string()),
                ),
                tx_count: maybe_init_spinner(
                    Cow::Borrowed(ConsensusMonitor::IDENT),
                    Cow::Owned(format!("Transaction Count (last {})", SNAPSHOT_INTERVAL_IN_SECS).to_string()),
                ),
                chain_block_count: maybe_init_spinner(
                    Cow::Borrowed(ConsensusMonitor::IDENT),
                    Cow::Owned(format!("Chain Block Count (last {})", SNAPSHOT_INTERVAL_IN_SECS).to_string()),
                ),
                dep_count: maybe_init_spinner(
                    Cow::Borrowed(ConsensusMonitor::IDENT),
                    Cow::Owned(format!("Dag Edges Count (last {})", SNAPSHOT_INTERVAL_IN_SECS).to_string()),
                ),
                mergeset_count: maybe_init_spinner(
                    Cow::Borrowed(ConsensusMonitor::IDENT),
                    Cow::Owned(format!("Mergeset Count (last {})", SNAPSHOT_INTERVAL_IN_SECS).to_string()),
                ),
                mass_count: maybe_init_spinner(
                    Cow::Borrowed(ConsensusMonitor::IDENT),
                    Cow::Owned(format!("Mass Count (last {})", SNAPSHOT_INTERVAL_IN_SECS).to_string()),
                ),
            });
        }
        None
    }
}

pub struct ConsensusMonitor {
    // Counters
    counters: Arc<ProcessingCounters>,

    // Tick service
    tick_service: Arc<TickService>,
}

impl ConsensusMonitor {
    pub const IDENT: &'static str = "ConsensusMonitor";

    pub fn new(counters: Arc<ProcessingCounters>, tick_service: Arc<TickService>) -> ConsensusMonitor {
        ConsensusMonitor { counters, tick_service }
    }

    pub async fn worker(self: &Arc<ConsensusMonitor>) {
        let mut last_snapshot = self.counters.snapshot();
        let mut last_log_time = Instant::now();
        let snapshot_interval = 10;
        loop {
            if let TickReason::Shutdown = self.tick_service.tick(Duration::from_secs(snapshot_interval)).await {
                // Let the system print final logs before exiting
                tokio::time::sleep(Duration::from_millis(500)).await;
                break;
            }

            let snapshot = self.counters.snapshot();
            if snapshot == last_snapshot {
                // No update, avoid printing useless info
                last_log_time = Instant::now();
                continue;
            }

            // Subtract the snapshots
            let delta = &snapshot - &last_snapshot;
            let now = Instant::now();

            info!(
                "Processed {} blocks and {} headers in the last {:.2}s ({} transactions; {} UTXO-validated blocks; {:.2} parents; {:.2} mergeset; {:.2} TPB; {:.1} mass)", 
                delta.body_counts,
                delta.header_counts,
                (now - last_log_time).as_secs_f64(),
                delta.txs_counts,
                delta.chain_block_counts,
                if delta.header_counts != 0 { delta.dep_counts as f64 / delta.header_counts as f64 } else { 0f64 },
                if delta.header_counts != 0 { delta.mergeset_counts as f64 / delta.header_counts as f64 } else { 0f64 },
                if delta.body_counts != 0 { delta.txs_counts as f64 / delta.body_counts as f64 } else{ 0f64 },
                if delta.body_counts != 0 { delta.mass_counts as f64 / delta.body_counts as f64 } else{ 0f64 },
            );

            last_snapshot = snapshot;
            last_log_time = now;
        }

        trace!("monitor thread exiting");
    }
}

// service trait implementation for Monitor
impl AsyncService for ConsensusMonitor {
    fn ident(self: Arc<Self>) -> &'static str {
        MONITOR
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", MONITOR);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", MONITOR);
            Ok(())
        })
    }
}
