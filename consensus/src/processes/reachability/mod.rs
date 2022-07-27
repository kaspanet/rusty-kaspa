mod extensions;
pub mod inquirer;
pub mod interval;
mod reindex;
pub mod tests;
mod tree;

use crate::model::stores::errors::StoreError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReachabilityError {
    #[error("data store error")]
    StoreError(#[from] StoreError),

    #[error("data overflow error")]
    DataOverflow,

    #[error("data inconsistency error")]
    DataInconsistency,

    #[error("query is inconsistent")]
    BadQuery,
}

pub type Result<T> = std::result::Result<T, ReachabilityError>;
