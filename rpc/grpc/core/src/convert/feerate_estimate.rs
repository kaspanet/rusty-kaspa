use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::RpcError;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcFeerateBucket, protowire::RpcFeerateBucket, {
    Self {
        feerate: item.feerate,
        estimated_seconds: item.estimated_seconds,
    }
});

from!(item: &kaspa_rpc_core::RpcFeeEstimate, protowire::RpcFeeEstimate, {
    Self {
        priority_bucket: Some((&item.priority_bucket).into()),
        normal_buckets: item.normal_buckets.iter().map(|b| b.into()).collect(),
        low_buckets: item.low_buckets.iter().map(|b| b.into()).collect(),
    }
});

from!(item: &kaspa_rpc_core::RpcFeeEstimateVerboseExperimentalData, protowire::RpcFeeEstimateVerboseExperimentalData, {
    Self {
        network_mass_per_second: item.network_mass_per_second,
        mempool_ready_transactions_count: item.mempool_ready_transactions_count,
        mempool_ready_transactions_total_mass: item.mempool_ready_transactions_total_mass,
        next_block_template_feerate_min: item.next_block_template_feerate_min,
        next_block_template_feerate_median: item.next_block_template_feerate_median,
        next_block_template_feerate_max: item.next_block_template_feerate_max,
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcFeerateBucket, kaspa_rpc_core::RpcFeerateBucket, {
    Self {
        feerate: item.feerate,
        estimated_seconds: item.estimated_seconds,
    }
});

try_from!(item: &protowire::RpcFeeEstimate, kaspa_rpc_core::RpcFeeEstimate, {
    Self {
        priority_bucket: item.priority_bucket
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcFeeEstimate".to_string(), "priority_bucket".to_string()))?
            .try_into()?,
        normal_buckets: item.normal_buckets.iter().map(|b| b.try_into()).collect::<Result<Vec<_>, _>>()?,
        low_buckets: item.low_buckets.iter().map(|b| b.try_into()).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(item: &protowire::RpcFeeEstimateVerboseExperimentalData, kaspa_rpc_core::RpcFeeEstimateVerboseExperimentalData, {
    Self {
        network_mass_per_second: item.network_mass_per_second,
        mempool_ready_transactions_count: item.mempool_ready_transactions_count,
        mempool_ready_transactions_total_mass: item.mempool_ready_transactions_total_mass,
        next_block_template_feerate_min: item.next_block_template_feerate_min,
        next_block_template_feerate_median: item.next_block_template_feerate_median,
        next_block_template_feerate_max: item.next_block_template_feerate_max,
    }
});
