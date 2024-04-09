use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ConfigError {
    #[error("Configuration: --addpeer and --connect cannot be used together")]
    MixedConnectAndAddPeers,

    #[error("Configuration: --logdir and --nologfiles cannot be used together")]
    MixedLogDirAndNoLogFiles,

    #[error("Configuration: --ram-scale cannot be set below 0.1")]
    RamScaleTooLow,

    #[error("Configuration: --ram-scale cannot be set above 10.0")]
    RamScaleTooHigh,

    #[error("Configuration: --max-tracked-addresses cannot be set above {0}")]
    MaxTrackedAddressesTooHigh(usize),

    #[cfg(feature = "devnet-prealloc")]
    #[error("Cannot preallocate UTXOs on any network except devnet")]
    PreallocUtxosOnNonDevnet,

    #[cfg(feature = "devnet-prealloc")]
    #[error("--num-prealloc-utxos has to appear with --prealloc-address and vice versa")]
    MissingPreallocNumOrAddress,
}

pub type ConfigResult<T> = std::result::Result<T, ConfigError>;
