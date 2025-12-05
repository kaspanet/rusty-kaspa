use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{FromRpcHex, RpcError, RpcHash, RpcResult, ToRpcHex};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcOptionalHeader, protowire::RpcOptionalHeader, {
    Self {
        version: item.version.map(|x| x.into()),
        hash: item.hash.map(|x| x.to_string()),
        parents_by_level: item.parents_by_level.iter().map(|x| x.as_slice().into()).collect(),
        hash_merkle_root: item.hash_merkle_root.map(|x| x.to_string()),
        accepted_id_merkle_root: item.accepted_id_merkle_root.map(|x| x.to_string()),
        utxo_commitment: item.utxo_commitment.map(|x| x.to_string()),
        timestamp: item.timestamp.map(|x| x as i64),
        bits: item.bits,
        nonce: item.nonce,
        daa_score: item.daa_score,
        blue_work: item.blue_work.map(|x| x.to_rpc_hex()),
        blue_score: item.blue_score,
        pruning_point: item.pruning_point.map(|x| x.to_string()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcOptionalHeader, kaspa_rpc_core::RpcOptionalHeader, {
    Self {
        version: item.version.map(|x| x as u16),
        hash: item.hash.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
        parents_by_level: item.parents_by_level.iter().map(Vec::<RpcHash>::try_from).collect::<RpcResult<Vec<Vec<RpcHash>>>>()?,
        hash_merkle_root: item.hash_merkle_root.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
        accepted_id_merkle_root: item.accepted_id_merkle_root.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
        utxo_commitment: item.utxo_commitment.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
        timestamp: item.timestamp.map(|x| x as u64),
        bits: item.bits,
        nonce: item.nonce,
        daa_score: item.daa_score,
        blue_work: item.blue_work.as_ref().map(|x| kaspa_rpc_core::RpcBlueWorkType::from_rpc_hex(x)).transpose()?,
        blue_score: item.blue_score,
        pruning_point: item.pruning_point.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
    }
});
