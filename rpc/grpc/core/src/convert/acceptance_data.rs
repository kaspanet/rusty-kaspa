use crate::protowire::{self};
use crate::{from, try_from};
use kaspa_rpc_core::{RpcError, RpcHash, RpcMergesetBlockAcceptanceData};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcAcceptanceData,  protowire::RpcAcceptanceData, {
    Self {
        accepting_chain_header: item.accepting_chain_header.as_ref().map(protowire::RpcBlockHeader::from),
        mergeset_block_acceptance_data: item
            .mergeset_block_acceptance_data
            .iter()
            .map(protowire::RpcMergesetBlockAcceptanceData::from)
            .collect(),
    }
});

from!(item: &kaspa_rpc_core::RpcMergesetBlockAcceptanceData, protowire::RpcMergesetBlockAcceptanceData, {
    Self {
        merged_block_hash: item.merged_block_hash.to_string(),
        accepted_transactions: item.accepted_transactions.iter().map(protowire::RpcTransaction::from).collect(),
    }
});

from!(item: &kaspa_rpc_core::RpcDataVerbosityLevel, protowire::RpcDataVerbosityLevel, {
    match item {
        kaspa_rpc_core::RpcDataVerbosityLevel::None => protowire::RpcDataVerbosityLevel::None,
        kaspa_rpc_core::RpcDataVerbosityLevel::Low => protowire::RpcDataVerbosityLevel::Low,
        kaspa_rpc_core::RpcDataVerbosityLevel::High => protowire::RpcDataVerbosityLevel::High,
        kaspa_rpc_core::RpcDataVerbosityLevel::Full => protowire::RpcDataVerbosityLevel::Full,
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcAcceptanceData, kaspa_rpc_core::RpcAcceptanceData, {
    Self {
        accepting_chain_header: item
            .accepting_chain_header
            .as_ref()
            .map(kaspa_rpc_core::RpcOptionalHeader::try_from)
            .transpose()?,
        mergeset_block_acceptance_data: item
        .mergeset_block_acceptance_data
        .iter()
        .map(RpcMergesetBlockAcceptanceData::try_from)
        .collect::<Result<_, _>>()?,
    }
});

try_from!(item: &protowire::RpcMergesetBlockAcceptanceData, kaspa_rpc_core::RpcMergesetBlockAcceptanceData, {
    Self {
        merged_block_hash: RpcHash::from_str(&item.merged_block_hash)?,
        accepted_transactions: item.accepted_transactions.iter().map(kaspa_rpc_core::RpcOptionalTransaction::try_from).collect::<Result<_, _>>()?,
    }
});

try_from!(item: &protowire::RpcDataVerbosityLevel, kaspa_rpc_core::RpcDataVerbosityLevel,  {
    match item {
        protowire::RpcDataVerbosityLevel::None => kaspa_rpc_core::RpcDataVerbosityLevel::None,
        protowire::RpcDataVerbosityLevel::Low => kaspa_rpc_core::RpcDataVerbosityLevel::Low,
        protowire::RpcDataVerbosityLevel::High => kaspa_rpc_core::RpcDataVerbosityLevel::High,
        protowire::RpcDataVerbosityLevel::Full => kaspa_rpc_core::RpcDataVerbosityLevel::Full
    }
});
