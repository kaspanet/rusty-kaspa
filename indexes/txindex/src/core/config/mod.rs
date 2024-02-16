mod constants;
pub mod params;
pub mod perf;

use std::sync::Arc;

use kaspa_consensus_core::config::Config as ConsensusConfig;

use crate::core::config::{params::Params, perf::PerfParams};

#[derive(Clone, Debug)]
pub struct Config {
    pub perf: PerfParams,
    pub params: Params,
}

impl From<&Arc<ConsensusConfig>> for Config {
    fn from(consensus_config: &Arc<ConsensusConfig>) -> Self {
        let params = Params::new(consensus_config);
        Self { params: params.clone(), perf: PerfParams::new(consensus_config, &params) }
    }
}
