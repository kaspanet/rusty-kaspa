use kaspa_database::prelude::StoreError;
use kaspa_hashes::Hash;
use kaspa_merkle::MerkleTreeError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReceiptsErrors {
    #[error("receipt import data lead to MerkleTreeError")]
    ReceiptsErrorImportMerkleTreeError(#[from] MerkleTreeError),
    #[error("receipt import data lead to storeError")]
    ReceiptsErrorImportStoreError(#[from] StoreError),
    #[error("posterity block for block {0} does not yet exist ")]
    PosterityDoesNotExistYet(Hash),
    #[error("Block with hash {0} is not on the selected chain, or was already pruned from the local database")]
    RequestedBlockNotOnSelectedChain(Hash),
    #[error("Block with hash {0} is orphaned with no chain blocks in its future")]
    NoChainBlockInFuture(Hash),
    #[error("tracked transaction {0} not found in acceptance data of block {1}")]
    TrackedTxNotFoundInAcceptanceData(Hash, Hash),
    #[error("invalid accepted tx index {index} in block {block_hash}")]
    InvalidAcceptedTxIndex { block_hash: Hash, index: u32 },
    #[error("lane {lane_key} is not active at posterity block {posterity_hash}")]
    LaneNotActiveAtPosterity { lane_key: Hash, posterity_hash: Hash },
}
