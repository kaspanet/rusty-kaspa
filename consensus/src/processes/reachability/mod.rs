mod extensions;
pub mod inquirer;
pub mod interval;
mod reindex;
pub mod tests;
mod tree;

use kaspa_database::prelude::StoreError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReachabilityError {
    #[error("data store error")]
    StoreError(#[from] StoreError),

    #[error("data overflow error")]
    DataOverflow(String),

    #[error("data inconsistency error")]
    DataInconsistency,

    #[error("query is inconsistent")]
    BadQuery,
}

impl kaspa_database::prelude::StoreErrorPredicates for ReachabilityError {
    fn is_key_not_found(&self) -> bool {
        matches!(self, ReachabilityError::StoreError(err) if err.is_key_not_found())
    }

    fn is_already_exists(&self) -> bool {
        matches!(self, ReachabilityError::StoreError(err) if err.is_already_exists())
    }
}

pub type Result<T> = std::result::Result<T, ReachabilityError>;
