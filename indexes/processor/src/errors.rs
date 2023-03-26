use kaspa_notify::events::EventType;
use kaspa_utxoindex::errors::UtxoIndexError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("{0}")]
    UtxoIndexError(#[from] UtxoIndexError),

    #[error("event type {0:?} is not supported")]
    NotSupported(EventType),
}
pub type IndexResult<T> = std::result::Result<T, IndexError>;
