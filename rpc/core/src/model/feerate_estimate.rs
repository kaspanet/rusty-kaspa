use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeerateBucket {
    /// The fee/mass ratio estimated to be required for inclusion time <= estimated_seconds
    pub feerate: f64,

    /// The estimated inclusion time for a transaction with fee/mass = feerate
    pub estimated_seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeEstimate {
    /// *Top-priority* feerate bucket. Provides an estimation of the feerate required for sub-second DAG inclusion.
    pub priority_bucket: FeerateBucket,

    /// A vector of *normal* priority feerate values. The first value of this vector is guaranteed to exist and
    /// provide an estimation for sub-*minute* DAG inclusion. All other values will have shorter estimation
    /// times than all `low_bucket` values. Therefor by chaining `[priority] | normal | low` and interpolating
    /// between them, one can compose a complete feerate function on the client side. The API makes an effort
    /// to sample enough "interesting" points on the feerate-to-time curve, so that the interpolation is meaningful.
    pub normal_buckets: Vec<FeerateBucket>,

    /// A vector of *low* priority feerate values. The first value of this vector is guaranteed to
    /// exist and provide an estimation for sub-*hour* DAG inclusion.
    pub low_buckets: Vec<FeerateBucket>,
}

impl FeeEstimate {
    pub fn ordered_buckets(&self) -> Vec<FeerateBucket> {
        std::iter::once(self.priority_bucket)
            .chain(self.normal_buckets.iter().copied())
            .chain(self.low_buckets.iter().copied())
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeEstimateVerboseExperimentalData {
    pub mempool_ready_transactions_count: u64,
    pub mempool_ready_transactions_total_mass: u64,
    pub network_mass_per_second: u64,

    pub next_block_template_feerate_min: f64,
    pub next_block_template_feerate_median: f64,
    pub next_block_template_feerate_max: f64,
}
