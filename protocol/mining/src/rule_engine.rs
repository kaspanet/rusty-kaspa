use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use kaspa_consensus_core::{
    api::counters::ProcessingCounters,
    network::NetworkType::{Mainnet, Testnet},
};
use kaspa_consensusmanager::ConsensusSessionOwned;
use kaspa_core::{
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    time::unix_now,
    trace, warn,
};
use kaspa_p2p_flows::flow_context::FlowContext;

const RULE_ENGINE: &str = "mining-rule-engine";
const SYNC_RATE_THRESHOLD: f64 = 0.10;

pub enum NearlySyncedFinder<'a> {
    BySession(&'a ConsensusSessionOwned),
    ByTimestampAndScore((u64, u64)),
}

#[derive(Clone)]
pub struct MiningRuleEngine {
    flow_context: Arc<FlowContext>,
    processing_counters: Arc<ProcessingCounters>,
    tick_service: Arc<TickService>,
    // Sync Rate Rule: Allow mining if sync rate is below threshold AND finality point is "recent" (defined below)
    use_sync_rate_rule: Arc<AtomicBool>,
}

impl MiningRuleEngine {
    pub async fn worker(self: &Arc<MiningRuleEngine>) {
        println!(module_path!());
        let snapshot_interval = 10;
        let mut last_snapshot = self.processing_counters.snapshot();
        let mut last_log_time = Instant::now();
        loop {
            if let TickReason::Shutdown = self.tick_service.tick(Duration::from_secs(snapshot_interval)).await {
                // Let the system print final logs before exiting
                tokio::time::sleep(Duration::from_millis(500)).await;
                break;
            }

            let now = Instant::now();
            let elapsed_time = now - last_log_time;
            if elapsed_time.as_secs() == 0 {
                continue;
            }

            let snapshot = self.processing_counters.snapshot();

            // Subtract the snapshots
            let delta = &snapshot - &last_snapshot;

            if elapsed_time.as_secs() > 0 {
                let expected_blocks = (elapsed_time.as_millis() as u64) / self.flow_context.config.target_time_per_block;
                let received_blocks = delta.body_counts.max(delta.header_counts);
                let rate: f64 = (received_blocks as f64) / (expected_blocks as f64);

                let session = self.flow_context.consensus().unguarded_session();

                let finality_point = session.async_finality_point().await;
                let finality_point_timestamp = session.async_get_header(finality_point).await.unwrap().timestamp;
                // Finality point is considered "recent" if it is within 3 finality durations from the current time
                let is_finality_recent =
                    finality_point_timestamp >= unix_now().saturating_sub(self.flow_context.config.finality_duration() * 3);

                trace!(
                    "Sync rate: {:.2} | Finality point recent: {} | Elapsed time: {}s | Found/Expected blocks: {}/{}",
                    rate,
                    is_finality_recent,
                    elapsed_time.as_secs(),
                    delta.body_counts,
                    expected_blocks,
                );

                if is_finality_recent && rate < SYNC_RATE_THRESHOLD {
                    // if sync rate rule conditions are met:
                    if let Ok(false) = self.use_sync_rate_rule.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed) {
                        warn!("Sync rate {:.2} is below threshold: {}", rate, SYNC_RATE_THRESHOLD);
                    }
                } else {
                    // else when sync rate conditions are not met:
                    if let Ok(true) = self.use_sync_rate_rule.compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed) {
                        if !is_finality_recent {
                            warn!("Sync rate {:.2} recovered: {} by entering IBD", rate, SYNC_RATE_THRESHOLD);
                        } else {
                            warn!("Sync rate {:.2} recovered: {}", rate, SYNC_RATE_THRESHOLD);
                        }
                    } else if !is_finality_recent {
                        trace!("Finality period is old. Timestamp: {}. Sync rate: {:.2}", finality_point_timestamp, rate);
                    }
                }
            }

            last_snapshot = snapshot;
            last_log_time = now;
        }
    }

    pub fn new(flow_context: Arc<FlowContext>, processing_counters: Arc<ProcessingCounters>, tick_service: Arc<TickService>) -> Self {
        Self { flow_context, processing_counters, tick_service, use_sync_rate_rule: Arc::new(AtomicBool::new(false)) }
    }

    pub async fn should_mine(&self, nearly_synced_finder: NearlySyncedFinder<'_>) -> bool {
        if !self.has_sufficient_peer_connectivity() {
            return false;
        }

        let is_nearly_synced = self.is_nearly_sycned(nearly_synced_finder).await;

        is_nearly_synced || self.use_sync_rate_rule.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn is_nearly_sycned(&self, nearly_synced_finder: NearlySyncedFinder<'_>) -> bool {
        match nearly_synced_finder {
            NearlySyncedFinder::ByTimestampAndScore((sink_timestamp, sink_daa_score)) => {
                self.flow_context.config.is_nearly_synced(sink_timestamp, sink_daa_score)
            }
            NearlySyncedFinder::BySession(session) => session.async_is_nearly_synced().await,
        }
    }

    fn has_sufficient_peer_connectivity(&self) -> bool {
        // Other network types can be used in an isolated environment without peers
        !matches!(self.flow_context.config.net.network_type, Mainnet | Testnet) || self.flow_context.hub().has_peers()
    }
}

impl AsyncService for MiningRuleEngine {
    fn ident(self: Arc<Self>) -> &'static str {
        RULE_ENGINE
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await;
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", RULE_ENGINE);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", RULE_ENGINE);
            Ok(())
        })
    }
}
