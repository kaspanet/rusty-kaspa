use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum ConversionError {
    #[error("General p2p conversion error")]
    General,

    #[error("Hash is None")]
    NoneHash,

    #[error("Bytes size mismatch error {0}")]
    ArrayBytesSizeError(#[from] std::array::TryFromSliceError),

    #[error("Bytes size mismatch error {0}")]
    UintBytesSizeError(#[from] math::uint::TryFromSliceError),

    #[error("Integer parsing error: {0}")]
    IntCastingError(#[from] std::num::TryFromIntError),
}
