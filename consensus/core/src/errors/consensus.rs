use hashes::Hash;
use thiserror::Error;

use super::{sync::SyncManagerError, traversal::TraversalError};

#[derive(Error, Debug, Clone)]
pub enum ConsensusError {
    #[error("couldn't find block {0}")]
    BlockNotFound(Hash),

    #[error("sync manager error")]
    SyncManagerError(#[from] SyncManagerError),

    #[error("traversal error")]
    TraversalError(#[from] TraversalError),

    #[error("{0}")]
    General(&'static str),
}

pub type ConsensusResult<T> = std::result::Result<T, ConsensusError>;
