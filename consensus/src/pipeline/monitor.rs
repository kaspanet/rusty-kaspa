use super::ProcessingCounters;
use indicatif::ProgressBar;
use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;
use kaspa_core::{
    info,
    log::progressions::{maybe_init_spinner, MULTI_PROGRESS_BAR_ACTIVE},
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace,
};
use kaspa_utils::option::OptionExtensions;
use std::{
    borrow::Cow,
    sync::Arc,
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
        if *MULTI_PROGRESS_BAR_ACTIVE {
            return Some(Self {
                header_count: maybe_init_spinner(Cow::Borrowed("Consensus"), Cow::Borrowed("Processed headers:"), false, true, true),
                block_count: maybe_init_spinner(
                    Cow::Borrowed("Consensus"),
                    Cow::Borrowed("Processed block bodies:"),
                    false,
                    true,
                    true,
                ),
                tx_count: maybe_init_spinner(Cow::Borrowed("Consensus"), Cow::Borrowed("Processed transactions:"), false, true, true),
                chain_block_count: maybe_init_spinner(
                    Cow::Borrowed("Consensus"),
                    Cow::Borrowed("Processed chain blocks:"),
                    false,
                    true,
                    true,
                ),
                dep_count: maybe_init_spinner(Cow::Borrowed("Consensus"), Cow::Borrowed("Processed DAG edges:"), false, true, true),
                mergeset_count: maybe_init_spinner(
                    Cow::Borrowed("Consensus"),
                    Cow::Borrowed("Processed mergesets:"),
                    false,
                    true,
                    true,
                ),
                mass_count: maybe_init_spinner(Cow::Borrowed("Consensus"), Cow::Borrowed("Processed tx mass:"), false, true, true),
            });
        }
        None
    }

    fn update_all(&self, counters: ProcessingCountersSnapshot) {
        self.header_count.is_some_perform(|pb| pb.set_position(counters.header_counts));
        self.block_count.is_some_perform(|pb| pb.set_position(counters.body_counts));
        self.tx_count.is_some_perform(|pb| pb.set_position(counters.txs_counts));
        self.chain_block_count.is_some_perform(|pb| pb.set_position(counters.chain_block_counts));
        self.dep_count.is_some_perform(|pb| pb.set_position(counters.dep_counts));
        self.mergeset_count.is_some_perform(|pb| pb.set_position(counters.mergeset_counts));
        self.mass_count.is_some_perform(|pb| pb.set_position(counters.mass_counts));
    }

    fn finish_all(&self) {
        self.header_count.is_some_perform(|pb| pb.finish());
        self.block_count.is_some_perform(|pb| pb.finish());
        self.tx_count.is_some_perform(|pb| pb.finish());
        self.chain_block_count.is_some_perform(|pb| pb.finish());
        self.dep_count.is_some_perform(|pb| pb.finish());
        self.mergeset_count.is_some_perform(|pb| pb.finish());
        self.mass_count.is_some_perform(|pb| pb.finish());
    }
}

pub struct ConsensusMonitor {
    // Counters
    counters: Arc<ProcessingCounters>,

    // Tick service
    tick_service: Arc<TickService>,

    // Progress bars
    progress_bars: Option<ConsensusProgressBars>,
}

impl ConsensusMonitor {
    pub const IDENT: &'static str = "ConsensusMonitor";

    pub fn new(counters: Arc<ProcessingCounters>, tick_service: Arc<TickService>) -> ConsensusMonitor {
        Self { counters, tick_service, progress_bars: ConsensusProgressBars::new() }
    }

    pub async fn worker(self: &Arc<ConsensusMonitor>) {
        let mut last_snapshot = self.counters.snapshot();
        let mut last_log_time = Instant::now();
        let log_snapshot_interval = Duration::from_secs(10);
        let mut last_progress_time = None;
        let mut progress_snapshot_interval = None;
        let mut snapshot_interval = log_snapshot_interval;

        if self.progress_bars.is_some() {
            progress_snapshot_interval = Some(Duration::from_millis(1000)); // we want finer granularity for progress bars
            last_progress_time = Some(Instant::now());
            snapshot_interval = log_snapshot_interval.min(progress_snapshot_interval.unwrap());
        }

        loop {
            if let TickReason::Shutdown = self.tick_service.tick(snapshot_interval).await {
                // Let the system print final logs before exiting
                tokio::time::sleep(Duration::from_millis(500)).await;
                self.progress_bars.is_some_perform(|pbs| pbs.finish_all());
                break;
            }

            let snapshot = self.counters.snapshot();
            let now = Instant::now();
            if snapshot == last_snapshot {
                // No update, avoid printing useless info
                last_log_time = now;
                last_progress_time = if self.progress_bars.is_some() { Some(now) } else { None };
                continue;
            }

            if let Some(ref mut last_progress_time) = last_progress_time {
                if let Some(progress_snapshot_interval) = progress_snapshot_interval {
                    if last_progress_time.elapsed() > progress_snapshot_interval {
                        self.progress_bars.is_some_perform(|pbs| pbs.update_all(snapshot.clone()));
                        *last_progress_time = now;
                    }
                }
            }

            if last_log_time.elapsed() > log_snapshot_interval {
                // Subtract the snapshots
                let delta = &snapshot - &last_snapshot;

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
                last_log_time = now;
            }
            last_snapshot = snapshot;
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
