use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::{Account, AddressBookEntry, PrvKeyData, PrvKeyDataId};
use kaspa_bip32::Mnemonic;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const PAYLOAD_VERSION: [u16; 3] = [1, 0, 0];

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    #[serde(default)]
    pub version: [u16; 3],

    pub prv_key_data: Vec<PrvKeyData>,
    pub accounts: Vec<Account>,
    pub address_book: Vec<AddressBookEntry>,
}

impl Payload {
    pub fn new(prv_key_data: Vec<PrvKeyData>, accounts: Vec<Account>, address_book: Vec<AddressBookEntry>) -> Self {
        Self { version: PAYLOAD_VERSION, prv_key_data, accounts, address_book }
    }
}

impl ZeroizeOnDrop for Payload {}

impl Zeroize for Payload {
    fn zeroize(&mut self) {
        self.prv_key_data.zeroize();
    }
}

impl Payload {
    pub fn add_prv_key_data(&mut self, mnemonic: Mnemonic, payment_secret: Option<&Secret>) -> Result<PrvKeyData> {
        let prv_key_data = PrvKeyData::try_new_from_mnemonic(mnemonic, payment_secret)?;

        if !self.prv_key_data.iter().any(|existing_key_data| prv_key_data.id == existing_key_data.id) {
            self.prv_key_data.push(prv_key_data.clone());
        } else {
            panic!("private key data id already exists in the wallet");
        }

        Ok(prv_key_data)
    }

    pub fn find_prv_key_data(&self, id: &PrvKeyDataId) -> Option<&PrvKeyData> {
        self.prv_key_data.iter().find(|prv_key_data| prv_key_data.id == *id)
    }
}
