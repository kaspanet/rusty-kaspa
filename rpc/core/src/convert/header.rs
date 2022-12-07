use crate::{RpcBlockLevelParents, RpcError, RpcHeader, RpcResult};
use consensus_core::header::Header;

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Header> for RpcHeader {
    fn from(item: &Header) -> Self {
        Self {
            hash: item.hash,
            version: item.version,
            parents: item.parents_by_level.iter().map(|x| RpcBlockLevelParents { parent_hashes: x.clone() }).collect(),
            hash_merkle_root: item.hash_merkle_root,
            accepted_id_merkle_root: item.accepted_id_merkle_root,
            utxo_commitment: item.utxo_commitment,
            timestamp: item.timestamp,
            bits: item.bits,
            nonce: item.nonce,
            daa_score: item.daa_score,
            blue_work: item.blue_work.into(),
            pruning_point: item.pruning_point,
            blue_score: item.blue_score,
        }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<&RpcHeader> for Header {
    type Error = RpcError;
    fn try_from(item: &RpcHeader) -> RpcResult<Self> {
        let header = Self::new(
            item.version,
            item.parents.iter().map(|x| x.parent_hashes.clone()).collect(),
            item.hash_merkle_root,
            item.accepted_id_merkle_root,
            item.utxo_commitment,
            item.timestamp,
            item.bits,
            item.nonce,
            item.daa_score,
            item.blue_work.into(),
            item.blue_score,
            item.pruning_point,
        );
        Ok(header)
    }
}
