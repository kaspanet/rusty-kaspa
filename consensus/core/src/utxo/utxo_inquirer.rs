use kaspa_hashes::Hash;
use thiserror::Error;

use crate::tx::{TransactionId, TransactionOutpoint};

#[derive(Error, Debug, Clone)]
pub enum UtxoInquirerError {
    #[error("Transaction return address is coinbase")]
    TxFromCoinbase,
    #[error("Transaction not found at given accepting daa score")]
    NoTxAtScore,
    #[error("Transaction was found but not standard")]
    NonStandard,
    #[error("No transaction specified")]
    TransactionNotFound,
    #[error("Did not find compact header for block hash {0} ")]
    MissingCompactHeaderForBlockHash(Hash),
    #[error("Did not find containing_acceptance for tx {0} ")]
    MissingContainingAcceptanceForTx(Hash),
    #[error("Did not find block {0} at block tx store")]
    MissingBlockFromBlockTxStore(Hash),
    #[error("Did not find index {0} in transactions of block {1}")]
    MissingTransactionIndexOfBlock(usize, Hash),
    #[error("Did not find a utxo diff for chain block {0} ")]
    MissingUtxoDiffForChainBlock(Hash),
    #[error("Transaction {0} acceptance data must also be in the same block in this case")]
    MissingOtherTransactionAcceptanceData(Hash),
    #[error("Did not find index for hash {0}")]
    MissingIndexForHash(Hash),
    #[error("Did not find tip data")]
    MissingTipData,
    #[error("Did not find a hash at index {0} ")]
    MissingHashAtIndex(u64),
    #[error("Did not find acceptance data for chain block {0}")]
    MissingAcceptanceDataForChainBlock(Hash),
    #[error("Did not find utxo entry for outpoint {0}")]
    MissingUtxoEntryForOutpoint(TransactionOutpoint),
    #[error("Did not find queried transactions in acceptance data: {0:?}")]
    MissingQueriedTransactions(Vec<TransactionId>),
    #[error("Utxo entry is not filled")]
    UnfilledUtxoEntry,
    #[error(transparent)]
    UtxoInquirerFindTxsFromAcceptanceDataError(#[from] UtxoInquirerFindTxsFromAcceptanceDataError),
}

#[derive(Error, Debug, Clone)]
pub enum UtxoInquirerFindTxsFromAcceptanceDataError {
    #[error("Tx ids filter is not allowed to be empty when not None.")]
    TxIdsFilterIsEmptyError,
    #[error("More than one tx id filter element is not supported yet.")]
    TxIdsFilterNeedsLessOrEqualThanOneElementError,
}

pub type UtxoInquirerResult<T> = std::result::Result<T, UtxoInquirerError>;
