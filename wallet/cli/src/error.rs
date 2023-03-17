use kaspa_wallet_core::error::Error as WalletError;
use workflow_core::channel::ChannelError;
use workflow_terminal::error::Error as TerminalError;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    String(String),

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
