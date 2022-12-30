use consensus_core::{
    errors::tx::TxRuleError,
    tx::{TransactionId, TransactionOutpoint},
};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum RuleError {
    #[error("")]
    RejectMalformed,

    #[error("")]
    RejectInvalid,

    #[error("")]
    RejectObsolete,

    #[error(transparent)]
    RejectTxRule(TxRuleError),

    /// RejectMissingOutpoint see domain\miningmanager\mempool\fill_inputs_and_get_missing_parents.go fillInputsAndGetMissingParents
    /// Should never be displayed since intercepted by validate_and_insert_transaction and turned into a call to maybe_orphan
    ///
    /// ok
    #[error("at least one outpoint of transaction is lacking a matching UTXO entry")]
    RejectMissingOutpoint,

    /// RejectDuplicate see domain\miningmanager\mempool\validate_transaction.go validateTransactionInIsolation
    /// "transaction %s is already in the mempool"
    ///
    /// ok
    #[error("transaction {0} is already in the mempool")]
    RejectDuplicate(TransactionId),

    /// RejectDuplicate see domain\miningmanager\mempool\mempool_utxo_set.go checkDoubleSpends
    ///
    /// ok
    #[error("output {0} already spent by transaction {1} in the memory pool")]
    RejectDoubleSpendInMempool(TransactionOutpoint, TransactionId),

    /// New error: see domain\miningmanager\mempool\transactions_pool.go limitTransactionCount
    /// New behavior: a transaction is rejected if the mempool is full
    #[error("number of high-priority transactions in mempool ({0}) is higher than maximum allowed ({1})")]
    RejectMempoolIsFull(usize, u64),

    #[error("")]
    RejectNotRequested,

    #[error("transaction {0} is not standard: {1}")]
    RejectNonStandard(TransactionId, String),

    #[error("")]
    RejectFinality,

    #[error("")]
    RejectDifficulty,

    /// RejectImmatureSpend see domain\miningmanager\mempool\fill_inputs_and_get_missing_parents.go fillInputsAndGetMissingParents
    ///
    /// ok
    #[error("one of the transaction inputs spends an immature UTXO: {0}")]
    RejectImmatureSpend(TxRuleError),

    #[error("transaction {0} doesn't exist in transaction pool")]
    RejectMissingTransaction(TransactionId),

    #[error("orphan transaction size of {0} bytes is larger than max allowed size of {1} bytes")]
    RejectBadOrphanMass(u64, u64),

    /// RejectDuplicate see domain\miningmanager\mempool\orphan_pool.go checkOrphanDuplicate
    #[error("Orphan transaction {0} is already in the orphan pool")]
    RejectDuplicateOrphan(TransactionId),

    /// RejectDuplicate see domain\miningmanager\mempool\orphan_pool.go checkOrphanDuplicate
    #[error("Orphan transaction {0} is double spending an input from already existing orphan {1}")]
    RejectDoubleSpendOrphan(TransactionId, TransactionId),

    /// RejectBadOrphan see domain\miningmanager\mempool\validate_and_insert_transaction.go, validateAndInsertTransaction
    /// "Transaction %s is an orphan, where allowOrphan = false"
    ///
    /// ok
    #[error("transaction {0} is an orphan where orphan is disallowed")]
    RejectDisallowedOrphan(TransactionId),

    /// RejectMissingOrphanOutpoint (added rule)
    /// see domain\miningmanager\mempool\orphan_pool.go, removeOrphan
    ///
    /// ok
    #[error("Input No. {0} of {1} ({2}) doesn't exist in orphan_ids_by_previous_outpoint")]
    RejectMissingOrphanOutpoint(usize, TransactionId, TransactionOutpoint),

    #[error("")]
    RejectBadOrphan,

    #[error("transaction {0} doesn't exist in orphan pool")]
    RejectMissingOrphanTransaction(TransactionId),
}

impl From<NonStandardError> for RuleError {
    fn from(item: NonStandardError) -> Self {
        RuleError::RejectNonStandard(*item.transaction_id(), item.to_string())
    }
}

impl From<TxRuleError> for RuleError {
    fn from(item: TxRuleError) -> Self {
        match item {
            TxRuleError::ImmatureCoinbaseSpend(_, _, _, _, _) => RuleError::RejectImmatureSpend(item),
            TxRuleError::MissingTxOutpoints => RuleError::RejectMissingOutpoint,
            _ => RuleError::RejectTxRule(item),
        }
    }
}

pub type RuleResult<T> = std::result::Result<T, RuleError>;

#[derive(Error, Debug, Clone)]
pub enum NonStandardError {
    /// RejectNonstandard see domain\miningmanager\mempool\check_transaction_standard.go checkTransactionStandardInIsolation
    /// "transaction version %d is not in the valid range of %d-%d"
    ///
    /// ok
    #[error("transaction version {1} is not in the valid range of {2}-{3}")]
    RejectVersion(TransactionId, u16, u16, u16),

    /// RejectNonstandard see domain\miningmanager\mempool\check_transaction_standard.go checkTransactionStandardInIsolation
    /// "transaction mass of %d is larger than max allowed size of %d"
    ///
    /// ok
    #[error("transaction mass of {1} is larger than max allowed size of {2}")]
    RejectMass(TransactionId, u64, u64),

    /// RejectNonstandard see domain\miningmanager\mempool\check_transaction_standard.go checkTransactionStandardInIsolation
    /// "transaction input %d: signature script size of %d bytes is larger than the maximum allowed size of %d bytes"
    ///
    /// ok
    #[error("transaction input #{1}: signature script size of {2} bytes is larger than the maximum allowed size of {3} bytes")]
    RejectSignatureScriptSize(TransactionId, usize, u64, u64),

    /// RejectNonstandard see domain\miningmanager\mempool\check_transaction_standard.go checkTransactionStandardInIsolation
    /// "The version of the scriptPublicKey is higher than the known version."
    ///
    /// ok
    #[error("transaction output #{1}: the version of the scriptPublicKey is higher than the known version")]
    RejectScriptPublicKeyVersion(TransactionId, usize),

    /// RejectNonstandard see domain\miningmanager\mempool\check_transaction_standard.go checkTransactionStandardInIsolation
    /// "transaction output %d: non-standard script form"
    ///
    /// ok
    #[error("transaction output #{1}: non-standard script form")]
    RejectOutputScriptClass(TransactionId, usize),

    /// RejectNonstandard see domain\miningmanager\mempool\check_transaction_standard.go checkTransactionStandardInIsolation
    /// "transaction output %d: payment of %d is dust"
    ///
    /// ok
    #[error("transaction output #{1}: payment of {2} is dust")]
    RejectDust(TransactionId, usize, u64),

    /// RejectNonstandard see domain\miningmanager\mempool\check_transaction_standard.go checkTransactionStandardInContext
    /// "transaction input #%d has a non-standard script form"
    ///
    /// ok
    #[error("transaction input {1}: non-standard script form")]
    RejectInputScriptClass(TransactionId, usize),

    /// RejectNonstandard see domain\miningmanager\mempool\check_transaction_standard.go checkTransactionStandardInIsolation & checkTransactionStandardInContext
    /// "transaction %s has %d fees which is under the required amount of %d"
    ///
    /// ok
    #[error("transaction has {1} fees which is under the required amount of {2}")]
    RejectInsufficientFee(TransactionId, u64, u64),

    /// RejectNonstandard see domain\miningmanager\mempool\check_transaction_standard.go checkTransactionStandardInContext
    /// "transaction input #%d has %d signature operations which is more than the allowed max amount of %d"
    ///
    /// ok
    #[error("transaction input #{1} has {2} signature operations which is more than the allowed max amount of {3}")]
    RejectSignatureCount(TransactionId, usize, u8, u8),
}

impl NonStandardError {
    pub fn transaction_id(&self) -> &TransactionId {
        match self {
            NonStandardError::RejectVersion(id, _, _, _) => id,
            NonStandardError::RejectMass(id, _, _) => id,
            NonStandardError::RejectSignatureScriptSize(id, _, _, _) => id,
            NonStandardError::RejectScriptPublicKeyVersion(id, _) => id,
            NonStandardError::RejectOutputScriptClass(id, _) => id,
            NonStandardError::RejectDust(id, _, _) => id,
            NonStandardError::RejectInputScriptClass(id, _) => id,
            NonStandardError::RejectInsufficientFee(id, _, _) => id,
            NonStandardError::RejectSignatureCount(id, _, _, _) => id,
        }
    }
}

pub type NonStandardResult<T> = std::result::Result<T, NonStandardError>;
