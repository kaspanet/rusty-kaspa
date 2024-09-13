use std::net::AddrParseError;

use downcast::DowncastError;
use kaspa_wallet_core::error::Error as WalletError;
use workflow_core::channel::ChannelError;
use workflow_terminal::error::Error as TerminalError;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("aborting")]
    UserAbort,

    #[error("platform is not supported")]
    Platform,

    #[error(transparent)]
    WalletError(#[from] WalletError),

    #[error("Cli error {0}")]
    TerminalError(#[from] TerminalError),

    #[error("Channel error")]
    ChannelError(String),

    #[error(transparent)]
    WrpcError(#[from] kaspa_wrpc_client::error::Error),

    #[error(transparent)]
    RpcError(#[from] kaspa_rpc_core::RpcError),

    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    ParseFloatError(#[from] std::num::ParseFloatError),

    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error("invalid hex string: {0}")]
    ParseHexError(#[from] faster_hex::Error),

    #[error(transparent)]
    AddrParseError(#[from] AddrParseError),

    #[error("account '{0}' not found")]
    AccountNotFound(String),

    #[error("ambiguous selection, pattern '{0}' matches too many accounts, please be more specific")]
    AmbiguousAccount(String),

    #[error("please create a wallet")]
    WalletDoesNotExist,

    #[error("please open a wallet")]
    WalletIsNotOpen,

    #[error("unrecognized argument '{0}', accepted arguments are: {1}")]
    UnrecognizedArgument(String, String),

    #[error("multiple matches for argument '{0}'; please be more specific.")]
    MultipleMatches(String),

    #[error("account type must be <bip32|multisig|legacy>")]
    InvalidAccountKind,

    #[error("wallet secret is required")]
    WalletSecretRequired,

    #[error("watch-only wallet kpub is required")]
    WalletBip32WatchXpubRequired,

    #[error("wallet secrets do not match")]
    WalletSecretMatch,

    #[error("payment secret is required")]
    PaymentSecretRequired,

    #[error("payment secrets do not match")]
    PaymentSecretMatch,

    #[error("key data not found")]
    KeyDataNotFound,

    #[error("no key data to export for watch-only account")]
    WatchOnlyAccountNoKeyData,

    #[error("no accounts found, please create an account to continue")]
    NoAccounts,

    #[error("no private keys found in this wallet, please create a private key to continue")]
    NoKeys,

    #[error(transparent)]
    AddressError(#[from] kaspa_addresses::AddressError),

    #[error("{0}")]
    DowncastError(String),

    #[error(transparent)]
    Store(#[from] workflow_store::error::Error),

    #[error(transparent)]
    NodeJs(#[from] workflow_node::error::Error),

    #[error(transparent)]
    Daemon(#[from] kaspa_daemon::error::Error),

    #[error(transparent)]
    Dom(#[from] workflow_dom::error::Error),

    #[error(transparent)]
    NetworkId(#[from] kaspa_consensus_core::network::NetworkIdError),

    #[error(transparent)]
    Bip32(#[from] kaspa_bip32::Error),

    #[error("private key {0} already exists")]
    PrivateKeyAlreadyExists(String),

    #[error(transparent)]
    MetricsError(kaspa_metrics_core::error::Error),

    #[error(transparent)]
    KaspaWalletKeys(#[from] kaspa_wallet_keys::error::Error),

    #[error(transparent)]
    PskbLockScriptSigError(#[from] kaspa_wallet_pskt::error::Error),

    #[error("To hex serialization error")]
    PskbSerializeToHexError,
}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
    }
}

impl From<Error> for TerminalError {
    fn from(e: Error) -> TerminalError {
        TerminalError::Custom(e.to_string())
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

impl<T> From<DowncastError<T>> for Error {
    fn from(e: DowncastError<T>) -> Self {
        Error::DowncastError(e.to_string())
    }
}
