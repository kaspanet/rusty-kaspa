use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
#[allow(unused_imports)]
use workflow_core::runtime;
use workflow_store::fs;

use crate::storage::{Decrypted, Encrypted, Hint, Metadata, Payload, PrvKeyData, PrvKeyDataId};

use crate::storage::local::Storage;

#[derive(Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub user_hint: Option<Hint>,
    pub payload: Encrypted,
    pub metadata: Vec<Metadata>,
}

impl Wallet {
    pub fn try_new(secret: Secret, payload: Payload) -> Result<Self> {
        let metadata =
            payload.accounts.iter().filter_map(|account| if account.is_visible { Some(account.clone()) } else { None }).collect();
        let payload = Decrypted::new(payload).encrypt(secret)?;
        Ok(Self { payload, metadata, user_hint: None })
    }

    pub fn payload(&self, secret: Secret) -> Result<Decrypted<Payload>> {
        self.payload.decrypt::<Payload>(secret)
    }

    pub async fn try_load(store: &Storage) -> Result<Wallet> {
        if fs::exists(store.filename()).await? {
            let wallet = fs::read_json::<Wallet>(store.filename()).await?;
            Ok(wallet)
        } else {
            Err(Error::NoWalletInStorage)
        }
    }

    pub async fn try_store_payload(store: &Storage, secret: Secret, payload: Payload) -> Result<()> {
        let wallet = Wallet::try_new(secret, payload)?;
        store.ensure_dir().await?;
        fs::write_json(store.filename(), &wallet).await?;
        Ok(())
    }

    pub async fn try_store(&self, store: &Storage) -> Result<()> {
        store.ensure_dir().await?;
        fs::write_json(store.filename(), self).await?;
        Ok(())
    }

    /// Obtain [`PrvKeyData`] by [`PrvKeyDataId`]
    pub async fn try_get_prv_key_data(&self, secret: Secret, prv_key_data_id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let payload = self.payload.decrypt::<Payload>(secret)?;
        let idx = payload.as_ref().prv_key_data.iter().position(|keydata| &keydata.id == prv_key_data_id);
        let keydata = idx.map(|idx| payload.as_ref().prv_key_data.get(idx).unwrap().clone());
        Ok(keydata)
    }
}
