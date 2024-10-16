use kaspa_database::prelude::StoreError;
use kaspa_hashes::Hash;
use kaspa_merkle::MerkleTreeError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReceiptsError {
    #[error("receipt import data lead to MerkleTreeError")]
    ReceiptsErrorImportMerkleTreeError(#[from] MerkleTreeError),
    #[error("receipt import data lead to storeError")]
    ReceiptsErrorImportStoreError(#[from] StoreError),
    #[error("Block with hash {0} does not yet have a post posterity block in database")]
    PostPosterityDoesNotExistYet(Hash),

}