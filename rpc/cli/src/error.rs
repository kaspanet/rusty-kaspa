//! CLI error type and sysexits-style exit-code mapping.

use thiserror::Error;

/// Errors surfaced by the CLI engine. Each variant maps to a process exit code
/// via [`CliError::exit_code`] following the `sysexits.h` conventions.
#[derive(Debug, Error)]
pub enum CliError {
    /// Bad invocation / arguments that clap did not catch.
    #[error("usage: {0}")]
    Usage(String),

    /// Configuration file or environment resolution failure.
    #[error("config: {0}")]
    Config(String),

    /// Could not establish a transport connection to the node.
    #[error("connection: {0}")]
    Connection(String),

    /// An RPC call returned an error.
    #[error(transparent)]
    Rpc(#[from] kaspa_rpc_core::RpcError),

    /// I/O failure (stdout, config file, completions, ...).
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON (de)serialization failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl From<toml::de::Error> for CliError {
    fn from(e: toml::de::Error) -> Self {
        CliError::Config(e.to_string())
    }
}

impl From<toml::ser::Error> for CliError {
    fn from(e: toml::ser::Error) -> Self {
        CliError::Config(e.to_string())
    }
}

impl CliError {
    /// Map to a `sysexits.h` exit code.
    /// - 64 `EX_USAGE`    : bad invocation / config.
    /// - 69 `EX_UNAVAILABLE`: connection / RPC failure.
    /// - 70 `EX_SOFTWARE` : internal (I/O, serde).
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Usage(_) | CliError::Config(_) => 64,
            CliError::Connection(_) | CliError::Rpc(_) => 69,
            CliError::Io(_) | CliError::Json(_) => 70,
        }
    }
}

/// Convenience result alias used across the crate.
pub type Result<T> = std::result::Result<T, CliError>;
