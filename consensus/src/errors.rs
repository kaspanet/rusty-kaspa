use crate::constants;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConsensusError {
    #[error("rule error")]
    RuleError(RuleError),

    #[error("unknown error")]
    Unknown(String),
}

#[derive(Error, Debug)]
pub enum RuleError {
    #[error("wrong block version: got {0} but expected {}", constants::BLOCK_VERSION)]
    WrongBlockVersion(u16),

    #[error(
        "the block timestamp is too much in the future: block timestamp is {0} but maximum timestamp allowed is {1}"
    )]
    TimeTooMuchInTheFuture(u64, u64),

    #[error("block has no parents")]
    NoParents,

    #[error("block has too many parents")]
    TooManyParents(u64),
}

pub type ConsensusResult<T> = std::result::Result<T, ConsensusError>;
