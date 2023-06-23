use crate::{block_template::BuilderError, mempool::RuleError};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum MiningManagerError {
    /// A consensus rule error
    #[error(transparent)]
    BlockTemplateBuilderError(#[from] BuilderError),

    /// A mempool rule error
    #[error(transparent)]
    MempoolError(#[from] RuleError),
}

pub type MiningManagerResult<T> = std::result::Result<T, MiningManagerError>;
