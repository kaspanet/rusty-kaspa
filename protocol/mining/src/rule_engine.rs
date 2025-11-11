use std::{
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use kaspa_consensus_core::{
    api::counters::ProcessingCounters,
    config::Config,
    daa_score_timestamp::DaaScoreTimestamp,
    mining_rules::MiningRules,
    network::NetworkType::{Mainnet, Testnet},
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    time::unix_now,
    trace,
};
use kaspa_p2p_lib::Hub;

use crate::rules::{mining_rule::MiningRule, sync_rate_rule::SyncRateRule, ExtraData};

const RULE_ENGINE: &str = "mining-rule-engine";
pub const SNAPSHOT_INTERVAL: u64 = 10;

#[derive(Clone)]
pub struct MiningRuleEngine {
    config: Arc<Config>,
    processing_counters: Arc<ProcessingCounters>,
    tick_service: Arc<TickService>,
    // Sync Rate Rule: Allow mining if sync rate is below threshold AND finality point is "recent" (defined below)
    use_sync_rate_rule: Arc<AtomicBool>,
    consensus_manager: Arc<ConsensusManager>,
    hub: Hub,
    mining_rules: Arc<MiningRules>,
    rules: Vec<Arc<dyn MiningRule>>,
}

impl MiningRuleEngine {
    pub async fn worker(self: &Arc<MiningRuleEngine>) {
        let mut last_snapshot = self.processing_counters.snapshot();
        let mut last_log_time = Instant::now();
        loop {
            // START: Sync monitor
            if let TickReason::Shutdown = self.tick_service.tick(Duration::from_secs(SNAPSHOT_INTERVAL)).await {
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
                let session = self.consensus_manager.consensus().unguarded_session();

                let finality_point = session.async_finality_point().await;
                let finality_point_timestamp = session.async_get_header(finality_point).await.unwrap().timestamp;

                let extra_data = ExtraData {
                    finality_point_timestamp,
                    target_time_per_block: self.config.target_time_per_block().after(),
                    has_sufficient_peer_connectivity: self.has_sufficient_peer_connectivity(),
                    finality_duration: self.config.finality_duration_in_milliseconds().after(),
                    elapsed_time,
                };

                trace!("Current Mining Rule: {:?}", self.mining_rules);

                // Check for all the rules
                for rule in &self.rules {
                    rule.check_rule(&delta, &extra_data);
                }
            }

            last_snapshot = snapshot;
            last_log_time = now;
        }
    }

    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        config: Arc<Config>,
        processing_counters: Arc<ProcessingCounters>,
        tick_service: Arc<TickService>,
        hub: Hub,
        mining_rules: Arc<MiningRules>,
    ) -> Self {
        let use_sync_rate_rule = Arc::new(AtomicBool::new(false));
        let rules: Vec<Arc<dyn MiningRule + 'static>> = vec![Arc::new(SyncRateRule::new(use_sync_rate_rule.clone()))];

        Self { consensus_manager, config, processing_counters, tick_service, hub, use_sync_rate_rule, mining_rules, rules }
    }

    pub fn should_mine(&self, sink_daa_score_timestamp: DaaScoreTimestamp) -> bool {
        if !self.has_sufficient_peer_connectivity() {
            return false;
        }

        let is_nearly_synced = self.is_nearly_synced(sink_daa_score_timestamp);

        is_nearly_synced || self.use_sync_rate_rule.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// In non-mining contexts, we consider the node synced if the sink is recent and it is connected
    /// to a peer
    pub fn is_sink_recent_and_connected(&self, sink_daa_score_timestamp: DaaScoreTimestamp) -> bool {
        self.has_sufficient_peer_connectivity() && self.is_nearly_synced(sink_daa_score_timestamp)
    }

    /// Returns whether the sink timestamp is recent enough and the node is considered synced or nearly synced.
    ///
    /// This info is used to determine if it's ok to use a block template from this node for mining purposes.
    pub fn is_nearly_synced(&self, sink_daa_score_timestamp: DaaScoreTimestamp) -> bool {
        let sink_timestamp = sink_daa_score_timestamp.timestamp;

        // We consider the node close to being synced if the sink (virtual selected parent) block
        // timestamp is within a quarter of the DAA window duration far in the past. Blocks mined over such DAG state would
        // enter the DAA window of fully-synced nodes and thus contribute to overall network difficulty
        //
        // [Crescendo]: both durations are nearly equal so this decision is negligible
        let synced_threshold = self.config.expected_difficulty_window_duration_in_milliseconds().after() / 4;

        // Roughly 10mins in all networks
        unix_now() < sink_timestamp + synced_threshold
    }

    fn has_sufficient_peer_connectivity(&self) -> bool {
        // Other network types can be used in an isolated environment without peers
        !matches!(self.config.net.network_type, Mainnet | Testnet) || self.hub.has_peers()
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
