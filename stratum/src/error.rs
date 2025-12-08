use thiserror::Error;

#[derive(Error, Debug)]
pub enum StratumError {
    #[error("Unknown error: {0}")]
    Unknown(String),

    #[error("Job not found")]
    JobNotFound,

    #[error("Duplicate share")]
    DuplicateShare,

    #[error("Low difficulty share")]
    LowDifficultyShare,

    #[error("Unauthorized worker")]
    UnauthorizedWorker,

    #[error("Not subscribed")]
    NotSubscribed,

    #[error("Block submission failed: {0}")]
    BlockSubmissionFailed(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Address error: {0}")]
    Address(#[from] kaspa_addresses::AddressError),
}

impl StratumError {
    pub fn code(&self) -> i32 {
        match self {
            StratumError::Unknown(_) => 20,
            StratumError::JobNotFound => 21,
            StratumError::DuplicateShare => 22,
            StratumError::LowDifficultyShare => 23,
            StratumError::UnauthorizedWorker => 24,
            StratumError::NotSubscribed => 25,
            StratumError::BlockSubmissionFailed(_) => 26,
            StratumError::Protocol(_) => 27,
            StratumError::Io(_) => 28,
            StratumError::Json(_) => 29,
            StratumError::Address(_) => 30,
        }
    }
}

