//!
//! Wallet address book.
//!

use crate::imports::*;

// TODO
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AddressBookEntry {
    pub alias: String,
    pub title: String,
    pub address: Address,
}
