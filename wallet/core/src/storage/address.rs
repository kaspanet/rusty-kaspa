use crate::imports::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressBookEntry {
    pub alias: String,
    pub title: String,
    pub address: Address,
}
