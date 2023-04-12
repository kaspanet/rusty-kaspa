use kaspa_hashes::Hash;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum SyncManagerError {
    #[error("low hash {0} is not in selected parent chain")]
    BlockNotInSelectedParentChain(Hash),

    #[error("low hash {0} is higher than high hash {1}")]
    LowHashHigherThanHighHash(Hash, Hash),

    #[error("pruning point {0} is not on selected parent chain of {1}")]
    PruningPointNotInChain(Hash, Hash),

    #[error("block locator low hash {0} is not on selected parent chain of high hash {1}")]
    LocatorLowHashNotInHighHashChain(Hash, Hash),
}

pub type SyncManagerResult<T> = std::result::Result<T, SyncManagerError>;
