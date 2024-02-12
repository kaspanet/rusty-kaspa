use kaspa_wrpc_client::error::Error as RpcError;
use thiserror::Error;
use toml::de::Error as TomlError;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError),

    #[error("TOML error: {0}")]
    Toml(#[from] TomlError),

    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    pub fn custom<T: ToString>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}
