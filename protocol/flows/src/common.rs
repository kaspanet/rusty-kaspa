use p2p_lib::core::ConnectionInitializationError;
use std::time::Duration;
use thiserror::Error;

/// Default P2P communication timeout
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120); // 2 minutes

#[derive(Error, Debug, Clone)]
pub enum FlowError {
    #[error("got timeout after {0:?}")]
    Timeout(Duration),

    #[error("expected {0} payload type but got {1:?}")]
    UnexpectedMessageType(&'static str, Box<Option<p2p_lib::pb::kaspad_message::Payload>>),

    #[error("channel is closed")]
    ChannelClosed,
}

impl From<FlowError> for ConnectionInitializationError {
    fn from(fe: FlowError) -> Self {
        match fe {
            FlowError::ChannelClosed => ConnectionInitializationError::ChannelClosed,
            err => ConnectionInitializationError::ProtocolError(err.to_string()),
        }
    }
}

/// Macro to extract a specific payload type from an `Option<pb::KaspadMessage>`.
/// Usage:
/// ```ignore
/// let res = extract_payload!(op, Payload::Verack)
/// ```
#[macro_export]
macro_rules! extract_payload {
    ($op:expr, $pattern:path) => {{
        if let Some(msg) = $op {
            if let Some($pattern(inner_msg)) = msg.payload {
                Ok(inner_msg)
            } else {
                Err($crate::common::FlowError::UnexpectedMessageType(stringify!($pattern), Box::new(msg.payload)))
            }
        } else {
            Err($crate::common::FlowError::ChannelClosed)
        }
    }};
}

/// Macro to await a channel `Receiver<pb::KaspadMessage>::recv` call with a specified/default timeout and expect a specific payload type.
/// Usage:
/// ```ignore
/// let res = recv_payload!(receiver, Payload::Verack)
/// // or:
/// let res = recv_payload!(receiver, Payload::Verack, Duration::from_secs(30))
/// ```
#[macro_export]
macro_rules! recv_payload {
    ($receiver:expr, $pattern:path) => {{
        match tokio::time::timeout($crate::common::DEFAULT_TIMEOUT, $receiver.recv()).await {
            Ok(op) => {
                $crate::extract_payload!(op, $pattern)
            }
            Err(_) => Err($crate::common::FlowError::Timeout($crate::common::DEFAULT_TIMEOUT)),
        }
    }};
    ($receiver:expr, $pattern:path, $timeout_duration:expr) => {{
        match tokio::time::timeout($timeout_duration, $receiver.recv()).await {
            Ok(op) => {
                $crate::extract_payload!(op, $pattern)
            }
            Err(_) => Err($crate::common::FlowError::Timeout($timeout_duration)),
        }
    }};
}
