//! See the accompanying fee_estimation.ipynb Jupyter Notebook which details the reasoning
//! behind this fee estimator.

use crate::block_template::selector::ALPHA;
use kaspa_utils::vec::VecExtensions;

/// The current standard minimum feerate (fee/mass = 1.0 is the current standard minimum).
/// TODO: pass from config
const MIN_FEERATE: f64 = 1.0;

#[derive(Clone, Copy, Debug)]
pub struct FeerateBucket {
    pub feerate: f64,
    pub estimated_seconds: f64,
}

#[derive(Clone, Debug)]
pub struct FeerateEstimations {
    /// *Top-priority* feerate bucket. Provides an estimation of the feerate required for sub-second DAG inclusion.
    pub priority_bucket: FeerateBucket,

    /// A vector of *normal* priority feerate values. The first value of this vector is guaranteed to
    /// provide an estimation for sub-*minute* DAG inclusion. All other values will have shorter estimation
    /// times than all `low_bucket` values. Therefor by chaining `[priority] | normal | low` and interpolating
    /// between them, once can compose a complete feerate function on the client side. The API makes an effort
    /// to sample enough "interesting" points on the feerate-to-time curve, so that the interpolation is meaningful.
    pub normal_buckets: Vec<FeerateBucket>,

    /// A vector of *low* priority feerate values. The first value of this vector is guaranteed to
    /// provide an estimation for sub-*hour* DAG inclusion.
    pub low_buckets: Vec<FeerateBucket>,
}

impl FeerateEstimations {
    pub fn ordered_buckets(&self) -> Vec<FeerateBucket> {
        vec![self.priority_bucket].merge(self.normal_buckets.clone()).merge(self.low_buckets.clone())
    }
}

pub struct FeerateEstimatorArgs {
    pub network_blocks_per_second: u64,
    pub maximum_mass_per_block: u64,
}

impl FeerateEstimatorArgs {
    pub fn network_mass_per_second(&self) -> u64 {
        self.network_blocks_per_second * self.maximum_mass_per_block
    }
}

pub struct FeerateEstimator {
    /// The total probability weight of all current mempool ready transactions, i.e., Σ_{tx in mempool}(tx.fee/tx.mass)^alpha
    total_weight: f64,

    /// The amortized time between transactions given the current transaction masses present in the mempool. Or in
    /// other words, the inverse of the transaction inclusion rate. For instance, if the average transaction mass is 2500 grams,
    /// the block mass limit is 500,000 and the network has 10 BPS, then this number would be 1/2000 seconds.
    inclusion_interval: f64,
}

impl FeerateEstimator {
    pub fn new(total_weight: f64, inclusion_interval: f64) -> Self {
        Self { total_weight, inclusion_interval }
    }

    pub(crate) fn feerate_to_time(&self, feerate: f64) -> f64 {
        let (c1, c2) = (self.inclusion_interval, self.total_weight);
        c1 * c2 / feerate.powi(ALPHA) + c1
    }

    fn time_to_feerate(&self, time: f64) -> f64 {
        let (c1, c2) = (self.inclusion_interval, self.total_weight);
        ((c1 * c2 / time) / (1f64 - c1 / time)).powf(1f64 / ALPHA as f64)
    }

    /// The antiderivative function of [`feerate_to_time`] excluding the constant shift `+ c1`
    fn feerate_to_time_antiderivative(&self, feerate: f64) -> f64 {
        let (c1, c2) = (self.inclusion_interval, self.total_weight);
        c1 * c2 / (-2f64 * feerate.powi(ALPHA - 1))
    }

    fn quantile(&self, lower: f64, upper: f64, frac: f64) -> f64 {
        assert!((0f64..=1f64).contains(&frac));
        let (c1, c2) = (self.inclusion_interval, self.total_weight);
        let z1 = self.feerate_to_time_antiderivative(lower);
        let z2 = self.feerate_to_time_antiderivative(upper);
        let z = frac * z2 + (1f64 - frac) * z1;
        ((c1 * c2) / (-2f64 * z)).powf(1f64 / (ALPHA - 1) as f64)
    }

    pub fn calc_estimations(&self) -> FeerateEstimations {
        let high = self.time_to_feerate(1f64).max(MIN_FEERATE);
        let low = self.time_to_feerate(3600f64).max(MIN_FEERATE).max(self.quantile(1f64, high, 0.25));
        let mid = self.time_to_feerate(60f64).max(MIN_FEERATE).max(self.quantile(low, high, 0.5));
        FeerateEstimations {
            priority_bucket: FeerateBucket { feerate: high, estimated_seconds: self.feerate_to_time(high) },
            normal_buckets: vec![FeerateBucket { feerate: mid, estimated_seconds: self.feerate_to_time(mid) }],
            low_buckets: vec![FeerateBucket { feerate: low, estimated_seconds: self.feerate_to_time(low) }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;

    #[test]
    fn test_feerate_estimations() {
        let estimator = FeerateEstimator { total_weight: 1002283.659, inclusion_interval: 0.004f64 };
        let estimations = estimator.calc_estimations();
        let buckets = estimations.ordered_buckets();
        for (i, j) in buckets.into_iter().tuple_windows() {
            assert!(i.feerate >= j.feerate);
        }
        dbg!(estimations);
    }

    #[test]
    fn test_min_feerate_estimations() {
        let estimator = FeerateEstimator { total_weight: 0.00659, inclusion_interval: 0.004f64 };
        let estimations = estimator.calc_estimations();
        let buckets = estimations.ordered_buckets();
        assert!(buckets.last().unwrap().feerate >= MIN_FEERATE);
        for (i, j) in buckets.into_iter().tuple_windows() {
            assert!(i.feerate >= j.feerate);
        }
    }
}
