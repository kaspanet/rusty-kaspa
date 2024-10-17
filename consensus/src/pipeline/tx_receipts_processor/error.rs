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
    #[error("Block with hash {0} does not yet have a post posterity block in database")]
    PostPosterityDoesNotExistYet(Hash),
    #[error("Block with hash {0} is not on the selected chain, or was already pruned from the local database")]
    RequestedBlockNotOnSelectedChain(Hash),
}
