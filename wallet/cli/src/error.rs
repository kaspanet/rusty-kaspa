use kaspa_wallet_core::error::Error as WalletError;
use workflow_core::channel::ChannelError;
use workflow_terminal::error::Error as TerminalError;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("Action aborted by the user")]
    UserAbort,

    #[error("Wallet error: {0}")]
    WalletError(#[from] WalletError),

    #[error("Cli error {0}")]
    TerminalError(#[from] TerminalError),
    // #[error("RPC error: {0}")]
    // RpcError(#[from] RpcError),
    #[error("Channel error")]
    ChannelError(String),
    // #[error("Channel error")]
    // ChannelError(String),
    #[error(transparent)]
    WrpcError(#[from] kaspa_wrpc_client::error::Error),

    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    ParseFloatError(#[from] std::num::ParseFloatError),

    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error("account '{0}' not found")]
    AccountNotFound(String),

    #[error("please create a wallet")]
    WalletDoesNotExist,

    #[error("please open a wallet")]
    WalletIsNotOpen,

    #[error("Unrecognized argument '{0}', accepted arguments are: {1}")]
    UnrecognizedArgument(String, String),

    #[error("multiple matches for argument '{0}'; please be more specific.")]
    MultipleMatches(String),

    #[error("account type must be <bip32|multisig|legacy>")]
    InvalidAccountKind,
}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
    }
}

impl From<Error> for TerminalError {
    fn from(e: Error) -> TerminalError {
        TerminalError::String(e.to_string())
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
