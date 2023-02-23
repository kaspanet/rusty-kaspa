use hashes::Hash;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ConsensusError {
    #[error("couldn't find block {0}")]
    BlockNotFound(Hash),

    #[error("{0}")]
    General(&'static str),
}

pub type ConsensusResult<T> = std::result::Result<T, ConsensusError>;
