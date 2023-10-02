use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::local::wallet::Wallet;
use crate::storage::local::wallet::WALLET_VERSION;
use crate::storage::local::*;
use crate::storage::*;
use std::collections::HashMap;

pub struct Cache {
    pub wallet_title: Option<String>,
    pub user_hint: Option<Hint>,
    pub prv_key_data: Encrypted,
    pub prv_key_data_info: Collection<PrvKeyDataId, PrvKeyDataInfo>,
    pub accounts: Collection<AccountId, Account>,
    pub metadata: Collection<AccountId, Metadata>,
    pub address_book: Vec<AddressBookEntry>,
}

impl TryFrom<(Wallet, &Secret)> for Cache {
    type Error = Error;
    fn try_from((wallet, secret): (Wallet, &Secret)) -> Result<Self> {
        let payload = wallet.payload(secret)?;

        let prv_key_data_info =
            payload.0.prv_key_data.iter().map(|pkdata| pkdata.into()).collect::<Vec<PrvKeyDataInfo>>().try_into()?;

        let prv_key_data_map = payload.0.prv_key_data.into_iter().map(|pkdata| (pkdata.id, pkdata)).collect::<HashMap<_, _>>();
        let prv_key_data: Decrypted<PrvKeyDataMap> = Decrypted::new(prv_key_data_map);
        let prv_key_data = prv_key_data.encrypt(secret)?;
        let accounts: Collection<AccountId, Account> = payload.0.accounts.try_into()?;
        let metadata: Collection<AccountId, Metadata> = wallet.metadata.try_into()?;
        let user_hint = wallet.user_hint;
        let wallet_title = wallet.title;
        let address_book = payload.0.address_book.into_iter().collect();

        Ok(Cache { wallet_title, user_hint, prv_key_data, prv_key_data_info, accounts, metadata, address_book })
    }
}

impl TryFrom<(Option<String>, Option<Hint>, Payload, &Secret)> for Cache {
    type Error = Error;
    fn try_from((wallet_title, user_hint, payload, secret): (Option<String>, Option<Hint>, Payload, &Secret)) -> Result<Self> {
        let prv_key_data_info = payload.prv_key_data.iter().map(|pkdata| pkdata.into()).collect::<Vec<PrvKeyDataInfo>>().try_into()?;

        let prv_key_data_map = payload.prv_key_data.into_iter().map(|pkdata| (pkdata.id, pkdata)).collect::<HashMap<_, _>>();
        let prv_key_data: Decrypted<PrvKeyDataMap> = Decrypted::new(prv_key_data_map);
        let prv_key_data = prv_key_data.encrypt(secret)?;
        let accounts: Collection<AccountId, Account> = payload.accounts.try_into()?;
        let metadata: Collection<AccountId, Metadata> = Collection::default();
        let address_book = payload.address_book.into_iter().collect();

        Ok(Cache { wallet_title, user_hint, prv_key_data, prv_key_data_info, accounts, metadata, address_book })
    }
}

impl TryFrom<(&Cache, &Secret)> for Wallet {
    type Error = Error;

    fn try_from((cache, secret): (&Cache, &Secret)) -> Result<Self> {
        let prv_key_data: Decrypted<PrvKeyDataMap> = cache.prv_key_data.decrypt(secret)?;
        let prv_key_data = prv_key_data.values().cloned().collect::<Vec<_>>();
        let accounts: Vec<Account> = (&cache.accounts).try_into()?;
        let metadata: Vec<Metadata> = (&cache.metadata).try_into()?;
        let address_book = cache.address_book.clone();
        let payload = Payload::new(prv_key_data, accounts, address_book);
        let payload = Decrypted::new(payload).encrypt(secret)?;

        Ok(Wallet {
            version: WALLET_VERSION,
            payload,
            metadata,
            user_hint: cache.user_hint.clone(),
            title: cache.wallet_title.clone(),
        })
    }
}
