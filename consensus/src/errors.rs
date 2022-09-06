use crate::constants;
use consensus_core::BlueWorkType;
use hashes::Hash;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum RuleError {
    #[error("wrong block version: got {0} but expected {}", constants::BLOCK_VERSION)]
    WrongBlockVersion(u16),

    #[error(
        "the block timestamp is too far into the future: block timestamp is {0} but maximum timestamp allowed is {1}"
    )]
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

    #[error("block difficulty of {0} is not the expected value of {1}")]
    UnexpectedDifficulty(u32, u32),

    #[error("block timestamp of {0} is not after expected {1}")]
    TimeTooOld(u64, u64),

    #[error("block is known to be invalid")]
    KnownInvalid,

    #[error("block merges {0} blocks > {1} merge set size limit")]
    MergeSetTooBig(u64, u64),

    #[error("block is violating bounded merge depth")]
    ViolatingBoundedMergeDepth,
}

pub type BlockProcessResult<T> = std::result::Result<T, RuleError>;
