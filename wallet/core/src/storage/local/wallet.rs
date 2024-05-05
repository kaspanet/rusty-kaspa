//!
//! Wallet data storage wrapper.
//!

use crate::imports::*;
use crate::storage::local::Payload;
use crate::storage::local::Storage;
use crate::storage::Encryptable;
use crate::storage::TransactionRecord;
use crate::storage::{AccountMetadata, Decrypted, Encrypted, Hint, PrvKeyData, PrvKeyDataId};
use workflow_store::fs;

#[derive(Clone, Serialize, Deserialize)]
pub struct WalletStorage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_hint: Option<Hint>,
    pub encryption_kind: EncryptionKind,
    pub payload: Encrypted,
    pub metadata: Vec<AccountMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions: Option<Encryptable<HashMap<AccountId, Vec<TransactionRecord>>>>,
}

impl WalletStorage {
    pub const STORAGE_MAGIC: u32 = 0x5753414b;
    pub const STORAGE_VERSION: u32 = 0;

    pub fn try_new(
        title: Option<String>,
        user_hint: Option<Hint>,
        secret: &Secret,
        encryption_kind: EncryptionKind,
        payload: Payload,
        metadata: Vec<AccountMetadata>,
    ) -> Result<Self> {
        let payload = Decrypted::new(payload).encrypt(secret, encryption_kind)?;
        Ok(Self { title, encryption_kind, payload, metadata, user_hint, transactions: None })
    }

    pub fn payload(&self, secret: &Secret) -> Result<Decrypted<Payload>> {
        self.payload.decrypt::<Payload>(secret).map_err(|err| match err {
            Error::Chacha20poly1305(e) => Error::WalletDecrypt(e),
            _ => err,
        })
    }

    pub async fn try_load(store: &Storage) -> Result<WalletStorage> {
        if fs::exists(store.filename()).await? {
            let bytes = fs::read(store.filename()).await?;
            Ok(BorshDeserialize::try_from_slice(bytes.as_slice())?)
        } else {
            let name = store.filename().file_name().unwrap().to_str().unwrap();
            Err(Error::NoWalletInStorage(name.to_string()))
        }
    }

    pub async fn try_store(&self, store: &Storage) -> Result<()> {
        store.ensure_dir().await?;

        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                let serialized = BorshSerialize::try_to_vec(self)?;
                fs::write(store.filename(), serialized.as_slice()).await?;
            } else {
                // make this platform-specific to avoid creating
                // a buffer containing serialization
                let mut file = std::fs::File::create(store.filename(), )?;
                BorshSerialize::serialize(self, &mut file)?;
            }
        }
        Ok(())
    }

    /// Obtain [`PrvKeyData`] using [`PrvKeyDataId`]
    pub async fn try_get_prv_key_data(&self, secret: &Secret, prv_key_data_id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let payload = self.payload.decrypt::<Payload>(secret)?;
        let idx = payload.as_ref().prv_key_data.iter().position(|keydata| &keydata.id == prv_key_data_id);
        let keydata = idx.map(|idx| payload.as_ref().prv_key_data.get(idx).unwrap().clone());
        Ok(keydata)
    }

    pub fn replace_metadata(&mut self, metadata: Vec<AccountMetadata>) {
        self.metadata = metadata;
    }
}

impl BorshSerialize for WalletStorage {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.title, writer)?;
        BorshSerialize::serialize(&self.user_hint, writer)?;
        BorshSerialize::serialize(&self.encryption_kind, writer)?;
        BorshSerialize::serialize(&self.payload, writer)?;
        BorshSerialize::serialize(&self.metadata, writer)?;
        BorshSerialize::serialize(&self.transactions, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for WalletStorage {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { magic, version, .. } = StorageHeader::deserialize(buf)?;

        if magic != Self::STORAGE_MAGIC {
            return Err(IoError::new(
                IoErrorKind::InvalidData,
                format!("This does not seem to be a kaspa wallet data file. Unknown file signature '0x{:x}'.", magic),
            ));
        }

        if version > Self::STORAGE_VERSION {
            return Err(IoError::new(
                IoErrorKind::InvalidData,
                format!("This wallet data was generated using a new version of the software. Please upgrade your software environment. Expected at most version '{}', encountered version '{}'", Self::STORAGE_VERSION, version),
            ));
        }

        let title = BorshDeserialize::deserialize(buf)?;
        let user_hint = BorshDeserialize::deserialize(buf)?;
        let encryption_kind = BorshDeserialize::deserialize(buf)?;
        let payload = BorshDeserialize::deserialize(buf)?;
        let metadata = BorshDeserialize::deserialize(buf)?;
        let transactions = BorshDeserialize::deserialize(buf)?;

        Ok(Self { title, user_hint, encryption_kind, payload, metadata, transactions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test_storage_wallet_storage() -> Result<()> {
        let storable_in = WalletStorage::try_new(
            Some("title".to_string()),
            Some(Hint::new("hint".to_string())),
            &Secret::from("secret"),
            EncryptionKind::XChaCha20Poly1305,
            Payload::new(vec![], vec![], vec![]),
            vec![],
        )?;
        let guard = StorageGuard::new(&storable_in);
        let _storable_out = guard.validate()?;

        Ok(())
    }
}
