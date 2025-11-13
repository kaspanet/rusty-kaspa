use kaspa_hashes::Hash;
use thiserror::Error;

use super::block::RuleError;

#[derive(Error, Debug, Clone)]
pub enum ArchivalError {
    #[error("child {0} was not found")]
    ChildNotFound(Hash),

    #[error("{0} is not a parent of {1}")]
    NotParentOf(Hash, Hash),

    #[error("node is not on archival mode")]
    NotArchival,

    #[error("rule error: {0}")]
    DifficultyError(#[from] RuleError),

    #[error("header of {0} was not found")]
    NoHeader(Hash),
}

pub type ArchivalResult<T> = std::result::Result<T, ArchivalError>;
