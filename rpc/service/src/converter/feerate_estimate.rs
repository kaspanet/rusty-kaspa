use kaspa_mining::feerate::{FeeEstimateVerbose, FeerateBucket, FeerateEstimations};
use kaspa_rpc_core::{
    message::GetFeeEstimateExperimentalResponse as RpcFeeEstimateVerboseResponse, RpcFeeEstimate,
    RpcFeeEstimateVerboseExperimentalData as RpcFeeEstimateVerbose, RpcFeerateBucket,
};

pub trait FeerateBucketConverter {
    fn into_rpc(self) -> RpcFeerateBucket;
}

impl FeerateBucketConverter for FeerateBucket {
    fn into_rpc(self) -> RpcFeerateBucket {
        RpcFeerateBucket { feerate: self.feerate, estimated_seconds: self.estimated_seconds }
    }
}

pub trait FeeEstimateConverter {
    fn into_rpc(self) -> RpcFeeEstimate;
}

impl FeeEstimateConverter for FeerateEstimations {
    fn into_rpc(self) -> RpcFeeEstimate {
        RpcFeeEstimate {
            priority_bucket: self.priority_bucket.into_rpc(),
            normal_buckets: self.normal_buckets.into_iter().map(FeerateBucketConverter::into_rpc).collect(),
            low_buckets: self.low_buckets.into_iter().map(FeerateBucketConverter::into_rpc).collect(),
        }
    }
}

pub trait FeeEstimateVerboseConverter {
    fn into_rpc(self) -> RpcFeeEstimateVerboseResponse;
}

impl FeeEstimateVerboseConverter for FeeEstimateVerbose {
    fn into_rpc(self) -> RpcFeeEstimateVerboseResponse {
        RpcFeeEstimateVerboseResponse {
            estimate: self.estimations.into_rpc(),
            verbose: Some(RpcFeeEstimateVerbose {
                network_mass_per_second: self.network_mass_per_second,
                mempool_ready_transactions_count: self.mempool_ready_transactions_count,
                mempool_ready_transactions_total_mass: self.mempool_ready_transactions_total_mass,
                next_block_template_feerate_min: self.next_block_template_feerate_min,
                next_block_template_feerate_median: self.next_block_template_feerate_median,
                next_block_template_feerate_max: self.next_block_template_feerate_max,
            }),
        }
    }
}
