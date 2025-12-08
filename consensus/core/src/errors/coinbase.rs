use kaspa_hashes::Hash;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum CoinbaseError {
    #[error("coinbase payload length is {0} while the minimum allowed length is {1}")]
    PayloadLenBelowMin(usize, usize),

    #[error("coinbase payload length is {0} while the maximum allowed length is {1}")]
    PayloadLenAboveMax(usize, usize),

    #[error("coinbase payload script public key length is {0} while the maximum allowed length is {1}")]
    PayloadScriptPublicKeyLenAboveMax(usize, u8),

    #[error("coinbase payload length is {0} bytes but it needs to be at least {1} bytes long in order to accommodate the script public key")]
    PayloadCantContainScriptPublicKey(usize, usize),

    #[error("missing reward data for block {0} (this may occur during IBD when reward data is not yet available)")]
    MissingRewardData(Hash),
}

pub type CoinbaseResult<T> = std::result::Result<T, CoinbaseError>;
