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
    #[error("wrong block version")]
    WrongBlockVersion(u64),

    #[error("the block timestamp is in the future")]
    TimeTooMuchInTheFuture(u64),

    #[error("block has no parents")]
    NoParents,

    #[error("block has too many parents")]
    TooManyParents(u64),
}

pub type ConsensusResult<T> = std::result::Result<T, ConsensusError>;
