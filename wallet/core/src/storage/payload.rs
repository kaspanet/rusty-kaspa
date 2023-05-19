use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::{Account, Encryptable, KeyDataPayload, PrvKeyData, PrvKeyDataId};
use kaspa_bip32::Mnemonic;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub prv_key_data: Vec<PrvKeyData>,
    pub accounts: Vec<Account>,
}

impl Payload {
    pub fn add_prv_key_data(&mut self, mnemonic: Mnemonic, payment_secret: Option<Secret>) -> Result<PrvKeyData> {
        let key_data_payload = KeyDataPayload::new(mnemonic.phrase().to_string());
        let key_data_payload_id = key_data_payload.id();
        let key_data_payload = Encryptable::Plain(key_data_payload);

        let mut prv_key_data = PrvKeyData::new(key_data_payload_id, key_data_payload);
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
