//! See the accompanying fee_estimation.ipynb Jupyter Notebook which details the reasoning
//! behind this fee estimator.

use crate::block_template::selector::ALPHA;
use itertools::Itertools;
use std::fmt::Display;

/// A type representing fee/mass of a transaction in `sompi/gram` units.
/// Given a feerate value recommendation, calculate the required fee by
/// taking the transaction mass and multiplying it by feerate: `fee = feerate * mass(tx)`
pub type Feerate = f64;

#[derive(Clone, Copy, Debug)]
pub struct FeerateBucket {
    pub feerate: f64,
    pub estimated_seconds: f64,
}

impl Display for FeerateBucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({:.4}, {:.4}s)", self.feerate, self.estimated_seconds)
    }
}

#[derive(Clone, Debug)]
pub struct FeerateEstimations {
    /// *Top-priority* feerate bucket. Provides an estimation of the feerate required for sub-second DAG inclusion.
    ///
    /// Note: for all buckets, feerate values represent fee/mass of a transaction in `sompi/gram` units.
    /// Given a feerate value recommendation, calculate the required fee by
    /// taking the transaction mass and multiplying it by feerate: `fee = feerate * mass(tx)`
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

impl FeerateEstimations {
    pub fn ordered_buckets(&self) -> Vec<FeerateBucket> {
        std::iter::once(self.priority_bucket)
            .chain(self.normal_buckets.iter().copied())
            .chain(self.low_buckets.iter().copied())
            .collect()
    }
}

impl Display for FeerateEstimations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(fee/mass, secs) priority: {}, ", self.priority_bucket)?;
        write!(f, "normal: {}, ", self.normal_buckets.iter().format(", "))?;
        write!(f, "low: {}", self.low_buckets.iter().format(", "))
    }
}

pub struct FeerateEstimatorArgs {
    pub network_blocks_per_second: u64,
    pub maximum_mass_per_block: u64,
}

impl FeerateEstimatorArgs {
    pub fn new(network_blocks_per_second: u64, maximum_mass_per_block: u64) -> Self {
        Self { network_blocks_per_second, maximum_mass_per_block }
    }

    pub fn network_mass_per_second(&self) -> u64 {
        self.network_blocks_per_second * self.maximum_mass_per_block
    }
}

#[derive(Debug, Clone)]
pub struct FeerateEstimator {
    /// The total probability weight of current mempool ready transactions, i.e., `Σ_{tx in mempool}(tx.fee/tx.mass)^alpha`.
    /// Note that some estimators might consider a reduced weight which excludes outliers. See [`Frontier::build_feerate_estimator`]
    total_weight: f64,

    /// The amortized time **in seconds** between transactions, given the current transaction masses present in the mempool. Or in
    /// other words, the inverse of the transaction inclusion rate. For instance, if the average transaction mass is 2500 grams,
    /// the block mass limit is 500,000 and the network has 10 BPS, then this number would be 1/2000 seconds.
    inclusion_interval: f64,
}

impl FeerateEstimator {
    pub fn new(total_weight: f64, inclusion_interval: f64) -> Self {
        assert!(total_weight >= 0.0);
        assert!((0f64..1f64).contains(&inclusion_interval));
        Self { total_weight, inclusion_interval }
    }

    pub(crate) fn feerate_to_time(&self, feerate: f64) -> f64 {
        let (c1, c2) = (self.inclusion_interval, self.total_weight);
        c1 * c2 / feerate.powi(ALPHA) + c1
    }

    fn time_to_feerate(&self, time: f64) -> f64 {
        let (c1, c2) = (self.inclusion_interval, self.total_weight);
        assert!(c1 < time, "{c1}, {time}");
        ((c1 * c2 / time) / (1f64 - c1 / time)).powf(1f64 / ALPHA as f64)
    }

    /// The antiderivative function of [`feerate_to_time`] excluding the constant shift `+ c1`
    #[inline]
    fn feerate_to_time_antiderivative(&self, feerate: f64) -> f64 {
        let (c1, c2) = (self.inclusion_interval, self.total_weight);
        c1 * c2 / (-2f64 * feerate.powi(ALPHA - 1))
    }

    /// Returns the feerate value for which the integral area is `frac` of the total area between `lower` and `upper`.
    fn quantile(&self, lower: f64, upper: f64, frac: f64) -> f64 {
        assert!((0f64..=1f64).contains(&frac));
        assert!(0.0 < lower && lower <= upper, "{lower}, {upper}");
        let (c1, c2) = (self.inclusion_interval, self.total_weight);
        if c1 == 0.0 || c2 == 0.0 {
            // if c1 · c2 == 0.0, the integral area is empty, so we simply return `lower`
            return lower;
        }
        let z1 = self.feerate_to_time_antiderivative(lower);
        let z2 = self.feerate_to_time_antiderivative(upper);
        // Get the total area corresponding to `frac` of the integral area between `lower` and `upper`
        // which can be expressed as z1 + frac * (z2 - z1)
        let z = frac * z2 + (1f64 - frac) * z1;
        // Calc the x value (feerate) corresponding to said area
        ((c1 * c2) / (-2f64 * z)).powf(1f64 / (ALPHA - 1) as f64)
    }

    pub fn calc_estimations(&self, minimum_standard_feerate: f64) -> FeerateEstimations {
        let min = minimum_standard_feerate;
        // Choose `high` such that it provides sub-second waiting time
        let high = self.time_to_feerate(1f64).max(min);
        // Choose `low` feerate such that it provides sub-hour waiting time AND it covers (at least) the 0.25 quantile
        let low = self.time_to_feerate(3600f64).max(self.quantile(min, high, 0.25));
        // Choose `normal` feerate such that it provides sub-minute waiting time AND it covers (at least) the 0.66 quantile between low and high.
        let normal = self.time_to_feerate(60f64).max(self.quantile(low, high, 0.66));
        // Choose an additional point between normal and low
        let mid = self.time_to_feerate(1800f64).max(self.quantile(min, high, 0.5));
        /* Intuition for the above:
               1. The quantile calculations make sure that we return interesting points on the `feerate_to_time` curve.
               2. They also ensure that the times don't diminish too high if small increments to feerate would suffice
                  to cover large fractions of the integral area (reflecting the position within the waiting-time distribution)
        */
        FeerateEstimations {
            priority_bucket: FeerateBucket { feerate: high, estimated_seconds: self.feerate_to_time(high) },
            normal_buckets: vec![
                FeerateBucket { feerate: normal, estimated_seconds: self.feerate_to_time(normal) },
                FeerateBucket { feerate: mid, estimated_seconds: self.feerate_to_time(mid) },
            ],
            low_buckets: vec![FeerateBucket { feerate: low, estimated_seconds: self.feerate_to_time(low) }],
        }
    }
}

#[derive(Clone, Debug)]
pub struct FeeEstimateVerbose {
    pub estimations: FeerateEstimations,

    pub mempool_ready_transactions_count: u64,
    pub mempool_ready_transactions_total_mass: u64,
    pub network_mass_per_second: u64,

    pub next_block_template_feerate_min: f64,
    pub next_block_template_feerate_median: f64,
    pub next_block_template_feerate_max: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;

    #[test]
    fn test_feerate_estimations() {
        let estimator = FeerateEstimator { total_weight: 1002283.659, inclusion_interval: 0.004f64 };
        let estimations = estimator.calc_estimations(1.0);
        let buckets = estimations.ordered_buckets();
        for (i, j) in buckets.into_iter().tuple_windows() {
            assert!(i.feerate >= j.feerate);
        }
        dbg!(estimations);
    }

    #[test]
    fn test_min_feerate_estimations() {
        let estimator = FeerateEstimator { total_weight: 0.00659, inclusion_interval: 0.004f64 };
        let minimum_feerate = 0.755;
        let estimations = estimator.calc_estimations(minimum_feerate);
        println!("{estimations}");
        let buckets = estimations.ordered_buckets();
        assert!(buckets.last().unwrap().feerate >= minimum_feerate);
        for (i, j) in buckets.into_iter().tuple_windows() {
            assert!(i.feerate >= j.feerate);
            assert!(i.estimated_seconds <= j.estimated_seconds);
        }
    }

    #[test]
    fn test_zero_values() {
        let estimator = FeerateEstimator { total_weight: 0.0, inclusion_interval: 0.0 };
        let minimum_feerate = 0.755;
        let estimations = estimator.calc_estimations(minimum_feerate);
        let buckets = estimations.ordered_buckets();
        for bucket in buckets {
            assert_eq!(minimum_feerate, bucket.feerate);
            assert_eq!(0.0, bucket.estimated_seconds);
        }

        let estimator = FeerateEstimator { total_weight: 0.0, inclusion_interval: 0.1 };
        let minimum_feerate = 0.755;
        let estimations = estimator.calc_estimations(minimum_feerate);
        let buckets = estimations.ordered_buckets();
        for bucket in buckets {
            assert_eq!(minimum_feerate, bucket.feerate);
            assert_eq!(estimator.inclusion_interval, bucket.estimated_seconds);
        }

        let estimator = FeerateEstimator { total_weight: 0.1, inclusion_interval: 0.0 };
        let minimum_feerate = 0.755;
        let estimations = estimator.calc_estimations(minimum_feerate);
        let buckets = estimations.ordered_buckets();
        for bucket in buckets {
            assert_eq!(minimum_feerate, bucket.feerate);
            assert_eq!(0.0, bucket.estimated_seconds);
        }
    }
}
