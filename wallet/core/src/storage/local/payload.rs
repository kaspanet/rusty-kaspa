use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::{Account, AddressBookEntry, PrvKeyData, PrvKeyDataId};
use kaspa_bip32::Mnemonic;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub prv_key_data: Vec<PrvKeyData>,
    pub accounts: Vec<Account>,
    pub address_book: Vec<AddressBookEntry>,
}

impl ZeroizeOnDrop for Payload {}

impl Zeroize for Payload {
    fn zeroize(&mut self) {
        self.prv_key_data.zeroize();
        // self.accounts.zeroize();
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
