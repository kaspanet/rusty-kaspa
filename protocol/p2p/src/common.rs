use crate::{convert::error::ConversionError, KaspadMessagePayloadType};
use kaspa_consensus_core::errors::{block::RuleError, consensus::ConsensusError, pruning::PruningImportError};
use kaspa_mining::errors::MiningManagerError;
use std::time::Duration;
use thiserror::Error;

/// Default P2P communication timeout
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120); // 2 minutes

#[derive(Error, Debug, Clone)]
pub enum ProtocolError {
    #[error("timeout expired after {0:?}")]
    Timeout(Duration),

    #[error("expected message type/s {0} but got {1:?}")]
    UnexpectedMessage(&'static str, Option<KaspadMessagePayloadType>),

    #[error("{0}")]
    ConversionError(#[from] ConversionError),

    #[error("{0}")]
    RuleError(#[from] RuleError),

    #[error("{0}")]
    PruningImportError(#[from] PruningImportError),

    #[error("{0}")]
    ConsensusError(#[from] ConsensusError),

    // TODO: discuss if such an error type makes sense here
    #[error("{0}")]
    MiningManagerError(#[from] MiningManagerError),

    #[error("{0}")]
    Other(&'static str),

    #[error("{0}")]
    OtherOwned(String),

    #[error("misbehaving peer: {0}")]
    MisbehavingPeer(String),

    #[error("peer connection is closed")]
    ConnectionClosed,
}

impl ProtocolError {
    pub fn is_connection_closed_error(&self) -> bool {
        matches!(self, Self::ConnectionClosed)
    }
}

/// Wraps an inner payload message into a valid `KaspadMessage`.
/// Usage:
/// ```ignore
/// let msg = make_message!(Payload::Verack, verack_msg)
/// ```
#[macro_export]
macro_rules! make_message {
    ($pattern:path, $msg:expr) => {{
        $crate::pb::KaspadMessage { payload: Some($pattern($msg)) }
    }};
}

/// Macro to extract a specific payload type from an `Option<pb::KaspadMessage>`.
/// Usage:
/// ```ignore
/// let res = unwrap_message!(op, Payload::Verack)
/// ```
#[macro_export]
macro_rules! unwrap_message {
    ($op:expr, $pattern:path) => {{
        if let Some(msg) = $op {
            if let Some($pattern(inner_msg)) = msg.payload {
                Ok(inner_msg)
            } else {
                Err($crate::common::ProtocolError::UnexpectedMessage(stringify!($pattern), msg.payload.as_ref().map(|v| v.into())))
            }
        } else {
            Err($crate::common::ProtocolError::ConnectionClosed)
        }
    }};
}

/// Macro to await a channel `Receiver<pb::KaspadMessage>::recv` call with a default/specified timeout and expect a specific payload type.
/// Usage:
/// ```ignore
/// let res = dequeue_with_timeout!(receiver, Payload::Verack) // Uses the default timeout
/// // or:
/// let res = dequeue_with_timeout!(receiver, Payload::Verack, Duration::from_secs(30))
/// ```
#[macro_export]
macro_rules! dequeue_with_timeout {
    ($receiver:expr, $pattern:path) => {{
        match tokio::time::timeout($crate::common::DEFAULT_TIMEOUT, $receiver.recv()).await {
            Ok(op) => {
                $crate::unwrap_message!(op, $pattern)
            }
            Err(_) => Err($crate::common::ProtocolError::Timeout($crate::common::DEFAULT_TIMEOUT)),
        }
    }};
    ($receiver:expr, $pattern:path, $timeout_duration:expr) => {{
        match tokio::time::timeout($timeout_duration, $receiver.recv()).await {
            Ok(op) => {
                $crate::unwrap_message!(op, $pattern)
            }
            Err(_) => Err($crate::common::ProtocolError::Timeout($timeout_duration)),
        }
    }};
}

/// Macro to indefinitely await a channel `Receiver<pb::KaspadMessage>::recv` call and expect a specific payload type (without a timeout).
/// Usage:
/// ```ignore
/// let res = dequeue!(receiver, Payload::Verack)
/// ```
#[macro_export]
macro_rules! dequeue {
    ($receiver:expr, $pattern:path) => {{
        $crate::unwrap_message!($receiver.recv().await, $pattern)
    }};
}
