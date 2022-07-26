mod extensions;
pub mod inquirer;
pub mod interval;
mod reindex;
pub(self) mod tests;

use thiserror::Error;

use crate::model::stores::errors::StoreError;

#[derive(Error, Debug)]
pub enum ReachabilityError {
    #[error("data store error")]
    ReachabilityStoreError(#[from] StoreError),

    #[error("data overflow error")]
    ReachabilityDataOverflowError,
}

pub type Result<T> = std::result::Result<T, ReachabilityError>;
