pub use crate::encryption::{Decrypted, Encryptable, Encrypted};

pub mod account;
pub mod address;
pub mod binding;
pub mod hint;
pub mod id;
pub mod interface;
pub mod keydata;
pub mod local;
pub mod metadata;
pub mod transaction;

pub use crate::runtime::{AccountId, AccountKind};
pub use account::Account;
pub use address::AddressBookEntry;
pub use binding::Binding;
pub use hint::Hint;
pub use id::IdT;
pub use interface::{AccessContextT, AccountStore, Interface, MetadataStore, PrvKeyDataStore, TransactionRecordStore};
pub use keydata::{KeyCaps, PrvKeyData, PrvKeyDataId, PrvKeyDataInfo, PrvKeyDataMap, PrvKeyDataPayload, PubKeyData, PubKeyDataId};
pub use metadata::Metadata;
pub use transaction::{TransactionMetadata, TransactionRecord, TransactionType};

#[cfg(test)]
mod tests {

    use super::*;
    use crate::result::Result;
    use crate::secret::Secret;
    use crate::storage::local::Payload;
    use crate::storage::local::Wallet;
    use kaspa_bip32::{Language, Mnemonic};

    #[tokio::test]
    async fn test_wallet_store_wallet_store_load() -> Result<()> {
        // This test creates a fake instance of keydata, stored account
        // instance and a wallet instance that owns them.  It then tests
        // loading of account references and a wallet instance and confirms
        // that the serialized data is as expected.

        let store = local::Storage::new("test-wallet-store")?;

        let mut payload = Payload::default();

        let wallet_secret = Secret::from("ABC-L4LXw2F7HEK3wJU-Rk4stbPy6c");
        let payment_secret = Secret::from("test-123-# L4LXw2F7HEK3wJU Rk4stbPy6c");
        let mnemonic1 = "caution guide valley easily latin already visual fancy fork car switch runway vicious polar surprise fence boil light nut invite fiction visa hamster coyote".to_string();
        let mnemonic2 = "nut invite fiction visa hamster coyote guide caution valley easily latin already visual fancy fork car switch runway vicious polar surprise fence boil light".to_string();

        let mnemonic1 = Mnemonic::new(mnemonic1, Language::English)?;
        let key_caps = KeyCaps::from_mnemonic_phrase(mnemonic1.phrase());
        let key_data_payload1 = PrvKeyDataPayload::try_new(mnemonic1.clone(), Some(&payment_secret))?;
        let prv_key_data1 = PrvKeyData::new(key_data_payload1.id(), None, key_caps, Encryptable::Plain(key_data_payload1));

        let mnemonic2 = Mnemonic::new(mnemonic2, Language::English)?;
        let key_caps = KeyCaps::from_mnemonic_phrase(mnemonic2.phrase());
        let key_data_payload2 = PrvKeyDataPayload::try_new(mnemonic2.clone(), Some(&payment_secret))?;
        let prv_key_data2 = PrvKeyData::new(
            key_data_payload2.id(),
            None,
            key_caps,
            Encryptable::Plain(key_data_payload2).into_encrypted(&payment_secret)?,
        );

        let pub_key_data1 = PubKeyData::new(vec!["abc".to_string()], None, None);
        let pub_key_data2 = PubKeyData::new(vec!["xyz".to_string()], None, None);
        println!("keydata1 id: {:?}", prv_key_data1.id);
        //assert_eq!(prv_key_data.id.0, [79, 36, 5, 159, 220, 113, 179, 22]);
        payload.prv_key_data.push(prv_key_data1.clone());
        payload.prv_key_data.push(prv_key_data2.clone());

        let account1 = Account::new(
            "Wallet-A".to_string(),
            "Wallet A".to_string(),
            AccountKind::Bip32,
            0,
            true,
            pub_key_data1.clone(),
            prv_key_data1.id,
            false,
            1,
            0,
        );
        //let account_id = account1.id.clone();
        payload.accounts.push(account1);

        let account2 = Account::new(
            "Wallet-B".to_string(),
            "Wallet B".to_string(),
            AccountKind::Bip32,
            0,
            true,
            pub_key_data2.clone(),
            prv_key_data2.id,
            false,
            1,
            0,
        );
        payload.accounts.push(account2);

        let payload_json = serde_json::to_string(&payload).unwrap();
        // let settings = WalletSettings::new(account_id);
        Wallet::try_store_payload(&store, &wallet_secret, payload).await?;

        let w2 = Wallet::try_load(&store).await?;
        let w2payload = w2.payload.decrypt::<Payload>(&wallet_secret).unwrap();
        println!("\n---\nwallet.metadata (plain): {:#?}\n\n", w2.metadata);
        // let w2payload_json = serde_json::to_string(w2payload.as_ref()).unwrap();
        println!("\n---nwallet.payload (decrypted): {:#?}\n\n", w2payload.as_ref());
        // purge the store
        store.purge().await?;

        assert_eq!(payload_json, serde_json::to_string(w2payload.as_ref())?);

        let w2keydata1 = w2payload.as_ref().prv_key_data.get(0).unwrap();
        let w2keydata1_payload = w2keydata1.payload.decrypt(None).unwrap();
        let first_mnemonic = &w2keydata1_payload.as_ref().as_mnemonic()?.unwrap().phrase_string();
        // println!("first mnemonic (plain): {}", hex_string(first_mnemonic.as_ref()));
        println!("first mnemonic (plain): {first_mnemonic}");
        assert_eq!(&mnemonic1.phrase_string(), first_mnemonic);

        let w2keydata2 = w2payload.as_ref().prv_key_data.get(1).unwrap();
        let w2keydata2_payload = w2keydata2.payload.decrypt(Some(&payment_secret)).unwrap();
        let second_mnemonic = &w2keydata2_payload.as_ref().as_mnemonic()?.unwrap().phrase_string();
        println!("second mnemonic (encrypted): {second_mnemonic}");
        assert_eq!(&mnemonic2.phrase_string(), second_mnemonic);

        Ok(())
    }
}
