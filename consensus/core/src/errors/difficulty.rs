use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum DifficultyError {
    #[error("under min allowed window size ({0} < {1})")]
    UnderMinWindowSizeAllowed(usize, usize),

    #[error("window data has only {0} entries -- this usually happens when the node has just began syncing")]
    InsufficientWindowData(usize),

    #[error("min window timestamp is equal to the max window timestamp")]
    EmptyTimestampRange,
}

pub type DifficultyResult<T> = std::result::Result<T, DifficultyError>;
