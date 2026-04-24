use thiserror::Error;

/// Common error type for the trusted‑relay crate.
#[derive(Debug, Error)]
pub enum RelayError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("peer connection error: {0}")]
    PeerConnection(String),

    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("channel send error: {0}")]
    ChannelSend(String),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("invalid packet: {0}")]
    InvalidPacket(String),
}

/// Convenience result alias used throughout the crate.
pub type RelayResult<T> = std::result::Result<T, RelayError>;
