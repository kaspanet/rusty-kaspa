pub mod args;
pub mod db;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    InvalidArgs(String),

    #[error("database error: {0}")]
    Database(#[from] kaspa_database::prelude::StoreError),

    #[error("failed opening RocksDB: {0}")]
    RocksDb(#[from] Box<dyn std::error::Error>),

    #[error("missing active consensus DB in meta database")]
    MissingActiveConsensus,
}
