// use downcast::DowncastError;
// use kaspa_wallet_core::error::Error as WalletError;
use workflow_core::channel::ChannelError;
// use workflow_terminal::error::Error as TerminalError;

use thiserror::Error;
use workflow_nw::ipc::ResponseError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("platform is not supported")]
    Platform,

    #[error("Channel error")]
    ChannelError(String),

    #[error(transparent)]
    Store(#[from] workflow_store::error::Error),

    #[error(transparent)]
    NodeJs(#[from] workflow_node::error::Error),

    #[error(transparent)]
    Ipc(#[from] workflow_nw::ipc::error::Error),
}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
    }
}

impl<T> From<ChannelError<T>> for Error {
    fn from(e: ChannelError<T>) -> Error {
        Error::ChannelError(e.to_string())
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::Custom(err)
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Self::Custom(err.to_string())
    }
}

impl From<Error> for ResponseError {
    fn from(e: Error) -> Self {
        ResponseError::Custom(e.to_string())
    }
}
