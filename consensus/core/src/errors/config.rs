use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ConfigError {
    #[error("Configuration: --addpeer and --connect cannot be used together")]
    MixedConnectAndAddPeers,

    #[error("Configuration: --logdir and --nologfiles cannot be used together")]
    MixedLogDirAndNoLogFiles,

    #[error("Cannot set fake UTXOs on any network except devnet")]
    FakeUTXOsOnNonDevnet,
}

pub type ConfigResult<T> = std::result::Result<T, ConfigError>;
