mod extensions;
pub mod inquirer;
pub mod interval;
mod reindex;

use thiserror::Error;

use crate::domain::consensus::model::stores::errors::StoreError;

#[derive(Error, Debug)]
pub enum ReachabilityError {
    #[error("data store error")]
    ReachabilityStoreError(#[from] StoreError),

    #[error("data overflow error")]
    ReachabilityDataOverflowError,
}

pub type Result<T> = std::result::Result<T, ReachabilityError>;
