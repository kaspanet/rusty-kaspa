//! Conversion of Block related types

use std::sync::Arc;

use crate::{RpcBlock, RpcError, RpcOptionalBlock, RpcOptionalTransaction, RpcRawBlock, RpcResult, RpcTransaction};
use kaspa_consensus_core::block::{Block, MutableBlock};

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Block> for RpcBlock {
    fn from(item: &Block) -> Self {
        Self {
            header: item.header.as_ref().into(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&Block> for RpcRawBlock {
    fn from(item: &Block) -> Self {
        Self { header: item.header.as_ref().into(), transactions: item.transactions.iter().map(RpcTransaction::from).collect() }
    }
}

impl From<&MutableBlock> for RpcBlock {
    fn from(item: &MutableBlock) -> Self {
        Self {
            header: item.header.as_ref().into(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            verbose_data: None,
        }
    }
}

impl From<&MutableBlock> for RpcRawBlock {
    fn from(item: &MutableBlock) -> Self {
        Self { header: item.header.as_ref().into(), transactions: item.transactions.iter().map(RpcTransaction::from).collect() }
    }
}

impl From<MutableBlock> for RpcRawBlock {
    fn from(item: MutableBlock) -> Self {
        Self { header: item.header.into(), transactions: item.transactions.iter().map(RpcTransaction::from).collect() }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcBlock) -> RpcResult<Self> {
        Ok(Self {
            header: Arc::new(item.header.try_into()?),
            transactions: Arc::new(
                item.transactions
                    .into_iter()
                    .map(kaspa_consensus_core::tx::Transaction::try_from)
                    .collect::<RpcResult<Vec<kaspa_consensus_core::tx::Transaction>>>()?,
            ),
        })
    }
}

impl TryFrom<RpcRawBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcRawBlock) -> RpcResult<Self> {
        Ok(Self {
            header: Arc::new(item.header.try_into()?),
            transactions: Arc::new(
                item.transactions
                    .into_iter()
                    .map(kaspa_consensus_core::tx::Transaction::try_from)
                    .collect::<RpcResult<Vec<kaspa_consensus_core::tx::Transaction>>>()?,
            ),
        })
    }
}

// ----------------------------------------------------------------------------
// consensus_core to optional rpc_core
// ----------------------------------------------------------------------------

impl From<&Block> for RpcOptionalBlock {
    fn from(item: &Block) -> Self {
        Self {
            header: Some(item.header.as_ref().into()),
            transactions: item.transactions.iter().map(RpcOptionalTransaction::from).collect(),
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&MutableBlock> for RpcOptionalBlock {
    fn from(item: &MutableBlock) -> Self {
        Self {
            header: Some(item.header.as_ref().into()),
            transactions: item.transactions.iter().map(RpcOptionalTransaction::from).collect(),
            verbose_data: None,
        }
    }
}

// ----------------------------------------------------------------------------
// optional rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcOptionalBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcOptionalBlock) -> RpcResult<Self> {
        Ok(Self {
            header: Arc::new(
                (item.header.ok_or(RpcError::MissingRpcFieldError("RpcBlock".to_string(), "header".to_string()))?).try_into()?,
            ),
            transactions: Arc::new(
                item.transactions
                    .into_iter()
                    .map(kaspa_consensus_core::tx::Transaction::try_from)
                    .collect::<RpcResult<Vec<kaspa_consensus_core::tx::Transaction>>>()?,
            ),
        })
    }
}
