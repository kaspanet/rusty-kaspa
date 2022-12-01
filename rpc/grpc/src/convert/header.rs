use crate::protowire;
use rpc_core::{RpcError, RpcHash, RpcResult};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

impl From<&rpc_core::RpcBlockHeader> for protowire::RpcBlockHeader {
    fn from(item: &rpc_core::RpcBlockHeader) -> Self {
        Self {
            version: item.version,
            parents: item.parents.iter().map(protowire::RpcBlockLevelParents::from).collect(),
            hash_merkle_root: item.hash_merkle_root.to_string(),
            accepted_id_merkle_root: item.accepted_id_merkle_root.to_string(),
            utxo_commitment: item.utxo_commitment.to_string(),
            timestamp: item.timestamp,
            bits: item.bits,
            nonce: item.nonce,
            daa_score: item.daa_score,
            blue_work: item.blue_work.to_string(),
            pruning_point: item.pruning_point.to_string(),
            blue_score: item.blue_score,
        }
    }
}

impl From<&rpc_core::RpcBlockLevelParents> for protowire::RpcBlockLevelParents {
    fn from(item: &rpc_core::RpcBlockLevelParents) -> Self {
        Self { parent_hashes: item.parent_hashes.iter().map(|x| x.to_string()).collect() }
    }
}

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

impl TryFrom<&protowire::RpcBlockHeader> for rpc_core::RpcBlockHeader {
    type Error = RpcError;
    fn try_from(item: &protowire::RpcBlockHeader) -> RpcResult<Self> {
        // TODO: determine if we need to calculate the hash here.
        // If so, do a rpc-core to consensus-core conversion to get the hash.
        Ok(Self {
            hash: Default::default(),
            version: item.version,
            parents: item
                .parents
                .iter()
                .map(rpc_core::RpcBlockLevelParents::try_from)
                .collect::<RpcResult<Vec<rpc_core::RpcBlockLevelParents>>>()?,
            hash_merkle_root: RpcHash::from_str(&item.hash_merkle_root)?,
            accepted_id_merkle_root: RpcHash::from_str(&item.accepted_id_merkle_root)?,
            utxo_commitment: RpcHash::from_str(&item.utxo_commitment)?,
            timestamp: item.timestamp,
            bits: item.bits,
            nonce: item.nonce,
            daa_score: item.daa_score,
            blue_work: rpc_core::RpcBlueWorkType::from_str(&item.blue_work)?,
            pruning_point: RpcHash::from_str(&item.pruning_point)?,
            blue_score: item.blue_score,
        })
    }
}

impl TryFrom<&protowire::RpcBlockLevelParents> for rpc_core::RpcBlockLevelParents {
    type Error = RpcError;
    fn try_from(item: &protowire::RpcBlockLevelParents) -> RpcResult<Self> {
        Ok(Self {
            parent_hashes: item
                .parent_hashes
                .iter()
                .map(|x| RpcHash::from_str(x))
                .collect::<Result<Vec<rpc_core::RpcHash>, faster_hex::Error>>()?,
        })
    }
}
