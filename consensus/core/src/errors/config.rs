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

    #[error("FEC data blocks (--fec-data-blocks) must be between 4 and 128")]
    FecDataBlocksOutOfRange,
    #[error("FEC parity blocks (--fec-parity-blocks) must be between 1 and 64")]
    FecParityBlocksOutOfRange,
    #[error("UDP payload size (--udp-payload-size) must be between 500 and 1472")]
    UdpPayloadSizeOutOfRange,
    #[error("Trusted relay requires a secret when incoming/outgoing peers are specified")]
    TrustedRelayMissingSecret,
}

pub type ConfigResult<T> = std::result::Result<T, ConfigError>;
