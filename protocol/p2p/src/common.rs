use crate::{KaspadMessagePayloadType, convert::error::ConversionError, core::peer::PeerKey};
use kaspa_consensus_core::errors::{block::RuleError, consensus::ConsensusError, pruning::PruningImportError};
use kaspa_mining_errors::manager::MiningManagerError;
use std::time::Duration;
use thiserror::Error;

/// Default P2P communication timeout
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120); // 2 minutes

#[derive(Error, Debug, Clone)]
pub enum ProtocolError {
    #[error("timeout expired after {0:?}")]
    Timeout(Duration),

    #[error("P2P protocol version mismatch - local: {0}, remote: {1}")]
    VersionMismatch(u32, u32),

    #[error("Network mismatch - local: {0}, remote: {1}")]
    WrongNetwork(String, String),

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
    IdentityError(#[from] uuid::Error),

    #[error("{0}")]
    Other(&'static str),

    #[error("{0}")]
    OtherOwned(String),

    #[error("misbehaving peer: {0}")]
    MisbehavingPeer(String),

    #[error("peer connection is closed")]
    ConnectionClosed,

    #[error("incoming route capacity for message type {0:?} has been reached (peer: {1})")]
    IncomingRouteCapacityReached(KaspadMessagePayloadType, String),

    #[error("outgoing route capacity has been reached (peer: {0})")]
    OutgoingRouteCapacityReached(String),

    #[error("no flow has been registered for message type {0:?}")]
    NoRouteForMessageType(KaspadMessagePayloadType),

    #[error("peer {0} already exists")]
    PeerAlreadyExists(PeerKey),

    #[error("loopback connection - node is connecting to itself")]
    LoopbackConnection(PeerKey),

    #[error("got reject message: {0}")]
    Rejected(String),

    #[error("got reject message: {0}")]
    IgnorableReject(String),
}

/// String used as a P2P convention to signal connection is rejected because we are connecting to ourselves
const LOOPBACK_CONNECTION_MESSAGE: &str = "LOOPBACK_CONNECTION";

/// String used as a P2P convention to signal connection is rejected because the peer already exists
const DUPLICATE_CONNECTION_MESSAGE: &str = "DUPLICATE_CONNECTION";

impl ProtocolError {
    pub fn is_connection_closed_error(&self) -> bool {
        matches!(self, Self::ConnectionClosed)
    }

    pub fn can_send_outgoing_message(&self) -> bool {
        !matches!(self, Self::ConnectionClosed | Self::OutgoingRouteCapacityReached(_))
    }

    pub fn to_reject_message(&self) -> String {
        match self {
            Self::LoopbackConnection(_) => LOOPBACK_CONNECTION_MESSAGE.to_owned(),
            Self::PeerAlreadyExists(_) => DUPLICATE_CONNECTION_MESSAGE.to_owned(),
            err => err.to_string(),
        }
    }

    pub fn from_reject_message(reason: String) -> Self {
        if reason == LOOPBACK_CONNECTION_MESSAGE || reason == DUPLICATE_CONNECTION_MESSAGE {
            ProtocolError::IgnorableReject(reason)
        } else if reason.contains("cannot find full block") {
            let hint = "Hint: If this error persists, it might be due to the other peer having pruned block data after syncing headers and UTXOs. In such a case, you may need to reset the database.";
            let detailed_reason = format!("{}. {}", reason, hint);
            ProtocolError::Rejected(detailed_reason)
        } else {
            ProtocolError::Rejected(reason)
        }
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
        $crate::pb::KaspadMessage {
            payload: Some($pattern($msg)),
            response_id: $crate::BLANK_ROUTE_ID,
            request_id: $crate::BLANK_ROUTE_ID,
        }
    }};

    ($pattern:path, $msg:expr, $response_id:expr, $request_id: expr) => {{ $crate::pb::KaspadMessage { payload: Some($pattern($msg)), response_id: $response_id, request_id: $request_id } }};
}

#[macro_export]
macro_rules! make_response {
    ($pattern:path, $msg:expr, $response_id:expr) => {{ $crate::pb::KaspadMessage { payload: Some($pattern($msg)), response_id: $response_id, request_id: 0 } }};
}

#[macro_export]
macro_rules! make_request {
    ($pattern:path, $msg:expr, $request_id:expr) => {{ $crate::pb::KaspadMessage { payload: Some($pattern($msg)), response_id: 0, request_id: $request_id } }};
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

#[macro_export]
macro_rules! unwrap_message_with_request_id {
    ($op:expr, $pattern:path) => {{
        if let Some(msg) = $op {
            if let Some($pattern(inner_msg)) = msg.payload {
                Ok((inner_msg, msg.request_id))
            } else {
                Err($crate::common::ProtocolError::UnexpectedMessage(stringify!($pattern), msg.payload.as_ref().map(|v| v.into())))
            }
        } else {
            Err($crate::common::ProtocolError::ConnectionClosed)
        }
    }};
}

#[macro_export]
macro_rules! unwrap_message_with_timestamp {
    ($op:expr, $pattern:path) => {{
        if let Some(msg) = $op {
            if let Some($pattern(inner_msg)) = msg.payload {
                Ok((inner_msg, Instant::now()))
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
    ($receiver:expr, $pattern:path) => {{ $crate::unwrap_message!($receiver.recv().await, $pattern) }};
}

#[macro_export]
macro_rules! dequeue_with_timestamp {
    ($receiver:expr, $pattern:path) => {{ $crate::unwrap_message_with_timestamp!($receiver.recv().await, $pattern) }};
}

#[macro_export]
macro_rules! dequeue_with_request_id {
    ($receiver:expr, $pattern:path) => {{ $crate::unwrap_message_with_request_id!($receiver.recv().await, $pattern) }};
}
