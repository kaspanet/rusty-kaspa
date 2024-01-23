//! Storable trait used to mark types that can be stored in the wallet storage.

use borsh::{BorshDeserialize, BorshSerialize};

/// Storable trait used to mark types that can be stored in the wallet storage.
pub trait Storable: Sized + BorshSerialize + BorshDeserialize {
    // a unique number used for binary
    // serialization data alignment check
    const STORAGE_MAGIC: u32;
    // AccountStorage binary serialization version
    const STORAGE_VERSION: u32;
}
