use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum TraversalError {
    #[error("passed max allowed traversal ({0} > {1})")]
    ReachedMaxTraversalAllowed(u64, u64),
}

pub type TraversalResult<T> = std::result::Result<T, TraversalError>;
