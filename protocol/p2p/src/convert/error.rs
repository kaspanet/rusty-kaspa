use kaspa_consensus_core::subnets::SubnetworkConversionError;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum ConversionError {
    #[error("General p2p conversion error")]
    General,

    #[error("Optional field is None while expected to be Some")]
    NoneValue,

    #[error("IP has illegal length {0}")]
    IllegalIPLength(usize),

    #[error("Bytes size mismatch error {0}")]
    ArrayBytesSizeError(#[from] std::array::TryFromSliceError),

    #[error("Bytes size mismatch error {0}")]
    UintBytesSizeError(#[from] kaspa_math::uint::TryFromSliceError),

    #[error("Integer parsing error: {0}")]
    IntCastingError(#[from] std::num::TryFromIntError),

    #[error(transparent)]
    AddressParsingError(#[from] std::net::AddrParseError),

    #[error(transparent)]
    IdentityError(#[from] uuid::Error),

    #[error(transparent)]
    SubnetParsingError(#[from] SubnetworkConversionError),

    #[error(transparent)]
    CompressedParentsError(#[from] kaspa_consensus_core::errors::header::CompressedParentsError),
}
