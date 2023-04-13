use std::sync::Arc;

use crate::{RpcBlock, RpcError, RpcResult, RpcTransaction};
use kaspa_consensus_core::block::{Block, MutableBlock};

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Block> for RpcBlock {
    fn from(item: &Block) -> Self {
        Self {
            header: (*item.header).clone(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&MutableBlock> for RpcBlock {
    fn from(item: &MutableBlock) -> Self {
        Self {
            header: item.header.clone(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            verbose_data: None,
        }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<&RpcBlock> for Block {
    type Error = RpcError;
    fn try_from(item: &RpcBlock) -> RpcResult<Self> {
        Ok(Self {
            header: Arc::new(item.header.clone()),
            transactions: Arc::new(
                item.transactions
                    .iter()
                    .map(kaspa_consensus_core::tx::Transaction::try_from)
                    .collect::<RpcResult<Vec<kaspa_consensus_core::tx::Transaction>>>()?,
            ),
        })
    }
}
