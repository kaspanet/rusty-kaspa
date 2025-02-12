use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;

pub trait MiningRule: Send + Sync + 'static {
    fn check_rule(&self, delta: &ProcessingCountersSnapshot);
}
