use crate::block_template::selector::ALPHA;

#[derive(Clone, Copy, Debug)]
pub struct FeerateBucket {
    pub feerate: f64,
    pub estimated_seconds: f64,
}

#[derive(Clone, Debug)]
pub struct FeerateEstimations {
    pub low_bucket: FeerateBucket,
    pub normal_bucket: FeerateBucket,
    pub priority_bucket: FeerateBucket,
}

#[allow(dead_code)] // TEMP (PR)
pub struct FeerateEstimator {
    /// The total probability weight of all current mempool ready transactions, i.e., Σ_{tx in mempool}(tx.fee/tx.mass)^alpha
    total_weight: f64,

    /// The amortized time between transactions given the current transaction masses present in the mempool, i.e.,
    /// the inverse of the transaction inclusion rate. For instance, if the average transaction mass is 2500 grams,
    /// the block mass limit is 500,000 and the network has 10 BPS, then this number would be 1/2000 seconds.
    inclusion_time: f64,
}

impl FeerateEstimator {
    fn feerate_to_time(&self, feerate: f64) -> f64 {
        let (c1, c2) = (self.inclusion_time, self.total_weight);
        c1 * c2 / feerate.powi(ALPHA) + c1
    }

    fn time_to_feerate(&self, time: f64) -> f64 {
        let (c1, c2) = (self.inclusion_time, self.total_weight);
        ((c1 * c2 / time) / (1f64 - c1 / time)).powf(1f64 / ALPHA as f64)
    }

    /// The antiderivative function of [`feerate_to_time`] excluding the constant shift `+ c1`
    fn feerate_to_time_antiderivative(&self, feerate: f64) -> f64 {
        let (c1, c2) = (self.inclusion_time, self.total_weight);
        c1 * c2 / (-2f64 * feerate.powi(ALPHA - 1))
    }

    fn quantile(&self, lower: f64, upper: f64, frac: f64) -> f64 {
        assert!((0f64..=1f64).contains(&frac));
        let (c1, c2) = (self.inclusion_time, self.total_weight);
        let z1 = self.feerate_to_time_antiderivative(lower);
        let z2 = self.feerate_to_time_antiderivative(upper);
        let z = frac * z2 + (1f64 - frac) * z1;
        ((c1 * c2) / (-2f64 * z)).powf(1f64 / (ALPHA - 1) as f64)
    }

    pub fn calc_estimations(&self) -> FeerateEstimations {
        let high = self.time_to_feerate(1f64);
        let low = self.time_to_feerate(3600f64).max(self.quantile(1f64, high, 0.25));
        let mid = self.time_to_feerate(60f64).max(self.quantile(low, high, 0.5));
        FeerateEstimations {
            low_bucket: FeerateBucket { feerate: low, estimated_seconds: self.feerate_to_time(low) },
            normal_bucket: FeerateBucket { feerate: mid, estimated_seconds: self.feerate_to_time(mid) },
            priority_bucket: FeerateBucket { feerate: high, estimated_seconds: self.feerate_to_time(high) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feerate_estimations() {
        let estimator = FeerateEstimator { total_weight: 1002283.659, inclusion_time: 0.004f64 };
        let estimations = estimator.calc_estimations();
        assert!(estimations.low_bucket.feerate <= estimations.normal_bucket.feerate);
        assert!(estimations.normal_bucket.feerate <= estimations.priority_bucket.feerate);
        assert!(estimations.low_bucket.estimated_seconds >= estimations.normal_bucket.estimated_seconds);
        assert!(estimations.normal_bucket.estimated_seconds >= estimations.priority_bucket.estimated_seconds);
        dbg!(estimations);
    }
}
