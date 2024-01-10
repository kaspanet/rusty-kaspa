use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum BlockStatus {
    /// StatusInvalid indicates that the block is invalid.
    StatusInvalid,

    /// StatusUTXOValid indicates the block is valid from any UTXO related aspects and has passed all the other validations as well.
    StatusUTXOValid,

    /// StatusUTXOPendingVerification indicates that the block is pending verification against its past UTXO-Set, either
    /// because it was not yet verified since the block was never in the selected parent chain, or if the
    /// block violates finality.
    StatusUTXOPendingVerification,

    /// StatusDisqualifiedFromChain indicates that the block is not eligible to be a selected parent.
    StatusDisqualifiedFromChain,

    /// StatusHeaderOnly indicates that the block transactions are not held (pruned or wasn't added yet)
    StatusHeaderOnly,
}

impl MemSizeEstimator for BlockStatus {}

impl BlockStatus {
    pub fn has_block_header(self) -> bool {
        matches!(
            self,
            Self::StatusHeaderOnly | Self::StatusUTXOValid | Self::StatusUTXOPendingVerification | Self::StatusDisqualifiedFromChain
        )
    }

    pub fn is_header_only(self) -> bool {
        self == Self::StatusHeaderOnly
    }

    pub fn has_block_body(self) -> bool {
        matches!(self, Self::StatusUTXOValid | Self::StatusUTXOPendingVerification | Self::StatusDisqualifiedFromChain)
    }

    pub fn is_utxo_valid_or_pending(self) -> bool {
        matches!(self, Self::StatusUTXOValid | Self::StatusUTXOPendingVerification)
    }

    pub fn is_valid(self) -> bool {
        self != BlockStatus::StatusInvalid
    }

    pub fn is_invalid(self) -> bool {
        self == BlockStatus::StatusInvalid
    }
}
