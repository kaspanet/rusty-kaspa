use std::collections::HashMap;

use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::{
    Account, AddressBookEntry, Encryptable, KeyCaps, PrvKeyData, PrvKeyDataId, PrvKeyDataPayload, TransactionMetadata,
    TransactionRecord, TransactionRecordId,
};
use kaspa_bip32::Mnemonic;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub prv_key_data: Vec<PrvKeyData>,
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub address_book: Vec<AddressBookEntry>,

    // ------
    pub transaction_records: Vec<TransactionRecord>,
    pub transaction_metadata: HashMap<TransactionRecordId, TransactionMetadata>,
}

impl ZeroizeOnDrop for Payload {}

impl Zeroize for Payload {
    fn zeroize(&mut self) {
        self.prv_key_data.zeroize();
        self.accounts.zeroize();
        // self.transaction_records.zeroize();
    }
}

impl Payload {
    pub fn add_prv_key_data(&mut self, mnemonic: Mnemonic, payment_secret: Option<&Secret>) -> Result<PrvKeyData> {
        let key_caps = KeyCaps::from_mnemonic_phrase(mnemonic.phrase());
        let key_data_payload = PrvKeyDataPayload::try_new(mnemonic, payment_secret)?;
        let key_data_payload_id = key_data_payload.id();
        let key_data_payload = Encryptable::Plain(key_data_payload);

        let mut prv_key_data = PrvKeyData::new(key_data_payload_id, None, key_caps, key_data_payload);
        if let Some(payment_secret) = payment_secret {
            prv_key_data.encrypt(payment_secret)?;
        }

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
