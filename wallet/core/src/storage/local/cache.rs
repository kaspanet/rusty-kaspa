use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::local::wallet::Wallet;
use crate::storage::local::*;
use crate::storage::*;
use std::collections::HashMap;

pub struct Cache {
    pub user_hint: Option<Hint>,
    pub prv_key_data: Encrypted,
    pub prv_key_data_info: Collection<PrvKeyDataId, PrvKeyDataInfo>,
    pub accounts: Collection<AccountId, Account>,
    pub metadata: Collection<AccountId, Metadata>,
    pub transaction_records: Collection<TransactionRecordId, TransactionRecord>,
}

impl TryFrom<(Wallet, &Secret)> for Cache {
    type Error = Error;
    fn try_from((wallet, secret): (Wallet, &Secret)) -> Result<Self> {
        let payload = wallet.payload(secret.clone())?;

        let prv_key_data_info =
            payload.0.prv_key_data.iter().map(|pkdata| pkdata.into()).collect::<Vec<PrvKeyDataInfo>>().try_into()?;

        let prv_key_data_map = payload.0.prv_key_data.into_iter().map(|pkdata| (pkdata.id, pkdata)).collect::<HashMap<_, _>>();
        let prv_key_data: Decrypted<PrvKeyDataMap> = Decrypted::new(prv_key_data_map);
        let prv_key_data = prv_key_data.encrypt(secret.clone())?;
        let accounts: Collection<AccountId, Account> = payload.0.accounts.try_into()?;
        let metadata: Collection<AccountId, Metadata> = wallet.metadata.try_into()?;
        let user_hint = wallet.user_hint;
        let transaction_records: Collection<TransactionRecordId, TransactionRecord> = payload.0.transaction_records.try_into()?;

        Ok(Cache { prv_key_data, prv_key_data_info, accounts, metadata, transaction_records, user_hint })
    }
}

impl TryFrom<(&Cache, &Secret)> for Wallet {
    type Error = Error;

    fn try_from((cache, secret): (&Cache, &Secret)) -> Result<Self> {
        let prv_key_data: Decrypted<PrvKeyDataMap> = cache.prv_key_data.decrypt(secret.clone())?;
        let prv_key_data = prv_key_data.values().cloned().collect::<Vec<_>>();
        let accounts: Vec<Account> = (&cache.accounts).try_into()?;
        let metadata: Vec<Metadata> = (&cache.metadata).try_into()?;
        let transaction_records: Vec<TransactionRecord> = (&cache.transaction_records).try_into()?;
        let payload = Payload { prv_key_data, accounts, transaction_records };
        let payload = Decrypted::new(payload).encrypt(secret.clone())?;

        Ok(Wallet { payload, metadata, user_hint: cache.user_hint.clone() })
    }
}
