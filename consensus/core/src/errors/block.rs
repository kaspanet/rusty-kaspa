use std::{collections::HashMap, fmt::Display};

use crate::{
    constants,
    errors::{coinbase::CoinbaseError, tx::TxRuleError},
    tx::{TransactionId, TransactionOutpoint},
    BlueWorkType,
};
use itertools::Itertools;
use kaspa_hashes::Hash;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct VecDisplay<T: Display>(pub Vec<T>);
impl<T: Display> Display for VecDisplay<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", self.0.iter().map(|item| item.to_string()).join(", "))
    }
}

#[derive(Clone, Debug)]
pub struct TwoDimVecDisplay<T: Display + Clone>(pub Vec<Vec<T>>);
impl<T: Display + Clone> Display for TwoDimVecDisplay<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[\n\t{}\n]", self.0.iter().cloned().map(|item| VecDisplay(item).to_string()).join(", \n\t"))
    }
}

#[derive(Error, Debug, Clone)]
pub enum RuleError {
    #[error("wrong block version: got {0} but expected {}", constants::BLOCK_VERSION)]
    WrongBlockVersion(u16),

    #[error("the block timestamp is too far into the future: block timestamp is {0} but maximum timestamp allowed is {1}")]
    TimeTooFarIntoTheFuture(u64, u64),

    #[error("block has no parents")]
    NoParents,

    #[error("block has too many parents: got {0} when the limit is {1}")]
    TooManyParents(usize, usize),

    #[error("block has ORIGIN as one of its parents")]
    OriginParent,

    #[error("parent {0} is an ancestor of parent {1}")]
    InvalidParentsRelation(Hash, Hash),

    #[error("parent {0} is invalid")]
    InvalidParent(Hash),

    #[error("block has missing parents: {0:?}")]
    MissingParents(Vec<Hash>),

    #[error("pruning point {0} is not in the past of this block")]
    PruningViolation(Hash),

    #[error("expected header daa score {0} but got {1}")]
    UnexpectedHeaderDaaScore(u64, u64),

    #[error("expected header blue score {0} but got {1}")]
    UnexpectedHeaderBlueScore(u64, u64),

    #[error("expected header blue work {0} but got {1}")]
    UnexpectedHeaderBlueWork(BlueWorkType, BlueWorkType),

    #[error("block {0} difficulty of {1} is not the expected value of {2}")]
    UnexpectedDifficulty(Hash, u32, u32),

    #[error("block timestamp of {0} is not after expected {1}")]
    TimeTooOld(u64, u64),

    #[error("block is known to be invalid")]
    KnownInvalid,

    #[error("block merges {0} blocks > {1} merge set size limit")]
    MergeSetTooBig(u64, u64),

    #[error("block is violating bounded merge depth")]
    ViolatingBoundedMergeDepth,

    #[error("invalid merkle root: header indicates {0} but calculated value is {1}")]
    BadMerkleRoot(Hash, Hash),

    #[error("block has no transactions")]
    NoTransactions,

    #[error("block first transaction is not coinbase")]
    FirstTxNotCoinbase,

    #[error("block has second coinbase transaction as index {0}")]
    MultipleCoinbases(usize),

    #[error("bad coinbase payload: {0}")]
    BadCoinbasePayload(CoinbaseError),

    #[error("coinbase blue score of {0} is not the expected value of {1}")]
    BadCoinbasePayloadBlueScore(u64, u64),

    #[error("transaction in isolation validation failed for tx {0}: {1}")]
    TxInIsolationValidationFailed(TransactionId, TxRuleError),

    #[error("block compute mass {0} exceeds limit of {1}")]
    ExceedsComputeMassLimit(u64, u64),

    #[error("block transient storage mass {0} exceeds limit of {1}")]
    ExceedsTransientMassLimit(u64, u64),

    #[error("block persistent storage mass {0} exceeds limit of {1}")]
    ExceedsStorageMassLimit(u64, u64),

    #[error("outpoint {0} is spent more than once on the same block")]
    DoubleSpendInSameBlock(TransactionOutpoint),

    #[error("outpoint {0} is created and spent on the same block")]
    ChainedTransaction(TransactionOutpoint),

    #[error("transaction in context validation failed for tx {0}: {1}")]
    TxInContextFailed(TransactionId, TxRuleError),

    #[error("wrong coinbase subsidy: expected {0} but got {1}")]
    WrongSubsidy(u64, u64),

    #[error("transaction {0} is found more than once in the block")]
    DuplicateTransactions(TransactionId),

    #[error("block has invalid proof-of-work")]
    InvalidPoW,

    #[error("expected header pruning point is {0} but got {1}")]
    WrongHeaderPruningPoint(Hash, Hash),

    #[error("expected indirect parents {0} but got {1}")]
    UnexpectedIndirectParents(TwoDimVecDisplay<Hash>, TwoDimVecDisplay<Hash>),

    #[error("block {0} UTXO commitment is invalid - block header indicates {1}, but calculated value is {2}")]
    BadUTXOCommitment(Hash, Hash, Hash),

    #[error("block {0} accepted ID merkle root is invalid - block header indicates {1}, but calculated value is {2}")]
    BadAcceptedIDMerkleRoot(Hash, Hash, Hash),

    #[error("coinbase transaction is not built as expected")]
    BadCoinbaseTransaction,

    #[error("{0} non-coinbase transactions (out of {1}) are invalid in UTXO context")]
    InvalidTransactionsInUtxoContext(usize, usize),

    #[error("invalid transactions in new block template")]
    InvalidTransactionsInNewBlock(HashMap<TransactionId, TxRuleError>),

    #[error("DAA window data has only {0} entries")]
    InsufficientDaaWindowSize(usize),

    /// Currently this error is never created because it is impossible to submit such a block
    #[error("cannot add block body to a pruned block")]
    PrunedBlock,
}

pub type BlockProcessResult<T> = std::result::Result<T, RuleError>;
