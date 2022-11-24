use crate::{RpcBlockHeader, RpcBlockLevelParents, RpcError, RpcResult};
use consensus_core::header::Header;

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Header> for RpcBlockHeader {
    fn from(item: &Header) -> Self {
        Self {
            version: item.version.into(),
            parents: item.parents_by_level.iter().map(|x| RpcBlockLevelParents { parent_hashes: x.clone() }).collect(),
            hash_merkle_root: item.hash_merkle_root,
            accepted_id_merkle_root: item.accepted_id_merkle_root,
            utxo_commitment: item.utxo_commitment,
            timestamp: item.timestamp.try_into().expect("time stamp is convertible from u64 to i64"),
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

impl TryFrom<&RpcBlockHeader> for Header {
    type Error = RpcError;
    fn try_from(item: &RpcBlockHeader) -> RpcResult<Self> {
        let mut header = Self::new(
            item.version.try_into()?,
            vec![],
            item.hash_merkle_root,
            item.timestamp.try_into()?,
            item.bits,
            item.nonce,
            item.daa_score,
            item.blue_work.into(),
            item.blue_score,
        );
        header.parents_by_level = item.parents.iter().map(|x| x.parent_hashes.clone()).collect();
        header.accepted_id_merkle_root = item.accepted_id_merkle_root;
        header.utxo_commitment = item.utxo_commitment;
        header.pruning_point = item.pruning_point;
        header.finalize();

        Ok(header)
    }
}
