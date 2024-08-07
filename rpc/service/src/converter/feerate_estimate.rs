use kaspa_mining::feerate::{FeerateBucket, FeerateEstimations};
use kaspa_rpc_core::{FeeEstimate as RpcFeeEstimate, FeerateBucket as RpcFeerateBucket};

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
