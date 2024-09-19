use kaspa_hashes::Hash;
use thiserror::Error;

use super::{difficulty::DifficultyError, sync::SyncManagerError, traversal::TraversalError};

#[derive(Error, Debug, Clone)]
pub enum ConsensusError {
    #[error("cannot find full block {0}")]
    BlockNotFound(Hash),

    #[error("cannot find header {0}")]
    HeaderNotFound(Hash),

    #[error("block {0} is invalid")]
    InvalidBlock(Hash),

    #[error("some data is missing for block {0}")]
    MissingData(Hash),

    #[error("got unexpected pruning point")]
    UnexpectedPruningPoint,

    #[error("pruning point is not at sufficient depth from virtual, cannot obtain its final anticone at this stage")]
    PruningPointInsufficientDepth,

    #[error("sync manager error: {0}")]
    SyncManagerError(#[from] SyncManagerError),

    #[error("traversal error: {0}")]
    TraversalError(#[from] TraversalError),

    #[error("difficulty error: {0}")]
    DifficultyError(#[from] DifficultyError),

    #[error("{0}")]
    General(&'static str),
}

pub type ConsensusResult<T> = std::result::Result<T, ConsensusError>;
