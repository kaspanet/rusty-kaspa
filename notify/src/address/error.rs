use kaspa_addresses::Address;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum Error {
    #[error("the address tracker reached the maximum capacity")]
    MaxCapacityReached,

    #[error("no prefix was attributed to the address tracker")]
    NoPrefix,

    #[error("address {0} does not match the address tracker prefix")]
    PrefixMismatch(Address),
}

pub type Result<T> = std::result::Result<T, Error>;
