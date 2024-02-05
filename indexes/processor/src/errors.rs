use kaspa_notify::events::EventType;
use kaspa_scoreindex::errors::ScoreIndexError;
use kaspa_utxoindex::errors::UtxoIndexError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("{0}")]
    UtxoIndexError(#[from] UtxoIndexError),

    #[error("{0}")]
    ScoreIndexError(#[from] ScoreIndexError),

    #[error("event type {0:?} is not supported")]
    NotSupported(EventType),
}
pub type IndexResult<T> = std::result::Result<T, IndexError>;
