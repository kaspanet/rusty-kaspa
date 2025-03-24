use kaspa_consensus_core::api::counters::ProcessingCountersSnapshot;

use super::ExtraData;

pub trait MiningRule: Send + Sync + 'static {
    fn check_rule(&self, delta: &ProcessingCountersSnapshot, extra_data: &ExtraData);
}
