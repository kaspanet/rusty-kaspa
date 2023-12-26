use borsh::{BorshDeserialize, BorshSerialize};

pub trait Storable: Sized + BorshSerialize + BorshDeserialize {
    // a unique number used for binary
    // serialization data alignment check
    const STORAGE_MAGIC: u32;
    // AccountStorage binary serialization version
    const STORAGE_VERSION: u32;
}
