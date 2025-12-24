use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum CompressedParentsError {
    #[error("Parents by level exceeds maximum levels of 255")]
    LevelsExceeded,
    #[error("Parents by level are not strictly increasing")]
    LevelsNotStrictlyIncreasing,
}

pub type CompressedParentsResult<T> = std::result::Result<T, CompressedParentsError>;
