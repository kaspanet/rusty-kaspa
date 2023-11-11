use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::local::Payload;
use crate::storage::local::Storage;
use crate::storage::{Decrypted, Encrypted, Hint, Metadata, PrvKeyData, PrvKeyDataId};
use serde_json::{from_str, from_value, Value};
use workflow_store::fs;

pub const WALLET_VERSION: [u16; 3] = [1, 0, 0];

#[derive(Clone, Serialize, Deserialize)]
pub struct Wallet {
    #[serde(default)]
    pub version: [u16; 3],

    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_hint: Option<Hint>,
    pub payload: Encrypted,
    pub metadata: Vec<Metadata>,
}

impl Wallet {
    pub fn try_new(
        title: Option<String>,
        user_hint: Option<Hint>,
        secret: &Secret,
        payload: Payload,
        metadata: Vec<Metadata>,
    ) -> Result<Self> {
        let payload = Decrypted::new(payload).encrypt(secret)?;
        Ok(Self { version: WALLET_VERSION, title, payload, metadata, user_hint })
    }

    pub fn payload(&self, secret: &Secret) -> Result<Decrypted<Payload>> {
        self.payload.decrypt::<Payload>(secret)
    }

    pub async fn try_load(store: &Storage) -> Result<Wallet> {
        if fs::exists(store.filename()).await? {
            let text = fs::read_to_string(store.filename()).await?;
            let root = from_str::<Value>(&text)?;

            let version = root.get("version");
            let version: [u16; 3] = if let Some(version) = version {
                from_value(version.clone()).map_err(|err| Error::Custom(format!("unknown wallet version `{version:?}`: {err}")))?
            } else {
                [0, 0, 0]
            };

            match version {
                [0,0,0] => {
                    Err(Error::Custom("wallet version 0.0.0 used during the development is no longer supported, please recreate the wallet using your saved mnemonic".to_string()))
                },
                _ => {
                    Ok(from_value::<Wallet>(root)?)
                }
            }
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

    /// Obtain [`PrvKeyData`] using [`PrvKeyDataId`]
    pub async fn try_get_prv_key_data(&self, secret: &Secret, prv_key_data_id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let payload = self.payload.decrypt::<Payload>(secret)?;
        let idx = payload.as_ref().prv_key_data.iter().position(|keydata| &keydata.id == prv_key_data_id);
        let keydata = idx.map(|idx| payload.as_ref().prv_key_data.get(idx).unwrap().clone());
        Ok(keydata)
    }

    pub fn replace_metadata(&mut self, metadata: Vec<Metadata>) {
        self.metadata = metadata;
    }
}
