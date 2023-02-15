use crate::{convert::error::ConversionError, ConnectionError};
use std::time::Duration;
use thiserror::Error;

/// Default P2P communication timeout
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120); // 2 minutes

#[derive(Error, Debug, Clone)]
pub enum FlowError {
    #[error("got timeout after {0:?}")]
    Timeout(Duration),

    #[error("expected {0} payload type but got {1:?}")]
    UnexpectedMessageType(&'static str, Box<Option<crate::pb::kaspad_message::Payload>>),

    #[error("{0}")]
    ProtocolError(&'static str),

    #[error("{0}")]
    ConversionError(#[from] ConversionError),

    #[error("inner connection error: {0}")]
    P2pConnectionError(ConnectionError),
}

impl From<FlowError> for ConnectionError {
    fn from(fe: FlowError) -> Self {
        match fe {
            FlowError::P2pConnectionError(err) => err,
            err => ConnectionError::ProtocolError(err.to_string()),
        }
    }
}

impl From<ConnectionError> for FlowError {
    fn from(err: ConnectionError) -> Self {
        FlowError::P2pConnectionError(err)
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
                Err($crate::common::FlowError::UnexpectedMessageType(stringify!($pattern), Box::new(msg.payload)))
            }
        } else {
            Err($crate::common::FlowError::P2pConnectionError($crate::ConnectionError::ChannelClosed))
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
            Err(_) => Err($crate::common::FlowError::Timeout($crate::common::DEFAULT_TIMEOUT)),
        }
    }};
    ($receiver:expr, $pattern:path, $timeout_duration:expr) => {{
        match tokio::time::timeout($timeout_duration, $receiver.recv()).await {
            Ok(op) => {
                $crate::unwrap_message!(op, $pattern)
            }
            Err(_) => Err($crate::common::FlowError::Timeout($timeout_duration)),
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
