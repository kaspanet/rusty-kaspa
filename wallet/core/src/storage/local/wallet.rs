use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::local::Payload;
use crate::storage::local::Storage;
use crate::storage::{Decrypted, Encrypted, Hint, Metadata, PrvKeyData, PrvKeyDataId};
use workflow_store::fs;

pub const WALLET_VERSION: [u16; 3] = [1, 0, 0];

#[derive(Clone, Serialize, Deserialize)]
pub struct Wallet {
    #[serde(default)]
    pub version: [u16; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_hint: Option<Hint>,
    pub payload: Encrypted,
    pub metadata: Vec<Metadata>,
}

impl Wallet {
    pub fn try_new(user_hint: Option<Hint>, secret: &Secret, payload: Payload) -> Result<Self> {
        let metadata = payload
            .accounts
            .iter()
            .filter_map(|account| if account.settings.is_visible { Some(account.clone()) } else { None })
            .collect();
        let payload = Decrypted::new(payload).encrypt(secret)?;
        Ok(Self { version: WALLET_VERSION, payload, metadata, user_hint })
    }

    pub fn payload(&self, secret: &Secret) -> Result<Decrypted<Payload>> {
        self.payload.decrypt::<Payload>(secret)
    }

    pub async fn try_load(store: &Storage) -> Result<Wallet> {
        if fs::exists(store.filename()).await? {
            let wallet = fs::read_json::<Wallet>(store.filename()).await?;
            Ok(wallet)
        } else {
            let name = store.filename().file_name().unwrap().to_str().unwrap();
            Err(Error::NoWalletInStorage(name.to_string()))
        }
    }

    pub async fn try_store(&self, store: &Storage) -> Result<()> {
        store.ensure_dir().await?;
        fs::write_json(store.filename(), self).await?;
        Ok(())
    }

    /// Obtain [`PrvKeyData`] by [`PrvKeyDataId`]
    pub async fn try_get_prv_key_data(&self, secret: &Secret, prv_key_data_id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let payload = self.payload.decrypt::<Payload>(secret)?;
        let idx = payload.as_ref().prv_key_data.iter().position(|keydata| &keydata.id == prv_key_data_id);
        let keydata = idx.map(|idx| payload.as_ref().prv_key_data.get(idx).unwrap().clone());
        Ok(keydata)
    }
}
