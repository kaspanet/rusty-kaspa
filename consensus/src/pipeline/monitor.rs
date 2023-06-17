use super::ProcessingCounters;
use kaspa_core::{
    info,
    task::service::{AsyncService, AsyncServiceFuture},
    trace,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const MONITOR: &str = "consensus-monitor";

pub struct ConsensusMonitor {
    // TODO: change the termination process using a chanel instead so we can (biased) select in the worker
    //       or use a shutdown-aware sleep service
    terminate: AtomicBool,
    // Counters
    counters: Arc<ProcessingCounters>,
}

impl ConsensusMonitor {
    pub fn new(counters: Arc<ProcessingCounters>) -> ConsensusMonitor {
        ConsensusMonitor { terminate: AtomicBool::new(false), counters }
    }

    pub async fn worker(self: &Arc<ConsensusMonitor>) {
        let mut last_snapshot = self.counters.snapshot();
        let mut last_log_time = Instant::now();
        let snapshot_interval = 10;
        loop {
            tokio::time::sleep(Duration::from_secs(snapshot_interval)).await;

            if self.terminate.load(Ordering::SeqCst) {
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
                "Processed {} blocks and {} headers in the last {:.2}s ({} transactions; {} parent references; {} UTXO-validated blocks; {:.2} avg txs per block; {} avg block mass)", 
                delta.body_counts,
                delta.header_counts,
                (now - last_log_time).as_secs_f64(),
                delta.txs_counts,
                delta.dep_counts,
                delta.chain_block_counts,
                if delta.body_counts != 0 { delta.txs_counts as f64 / delta.body_counts as f64 } else{ 0f64 },
                if delta.body_counts != 0 { delta.mass_counts / delta.body_counts } else{ 0 },
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
        self.terminate.store(true, Ordering::SeqCst);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        trace!("{} stopping", MONITOR);
        Box::pin(async move {
            trace!("{} exiting", MONITOR);
            Ok(())
        })
    }
}
