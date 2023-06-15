use std::collections::HashMap;

use crate::imports::*;
use crate::iterator::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::local::iterators::*;
use crate::storage::local::wallet::Wallet;
use crate::storage::local::*;
use crate::storage::*;
use async_trait::async_trait;

#[derive(Default)]
pub struct Cache {
    pub user_hint: Option<String>,
    pub prv_key_data: Encrypted,
    // pub prv_key_data_info: Vec<Arc<PrvKeyDataInfo>>,
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
        let transaction_records : Collection<TransactionRecordId,TransactionRecord> = payload.0.transaction_records.try_into()?;

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

pub(crate) struct LocalStoreInner {
    pub cache: Mutex<Cache>,
    pub store: Store,
}

impl LocalStoreInner {
    pub fn try_new(folder: Option<&str>, name: &str) -> Result<Self> {
        let store = Store::new(folder.unwrap_or(super::DEFAULT_STORAGE_FOLDER), name)?;
        Ok(Self { cache: Mutex::new(Cache::default()), store })
    }

    pub fn cache(&self) -> MutexGuard<Cache> {
        self.cache.lock().unwrap()
    }

    pub async fn reload(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        let secret = ctx.wallet_secret().await.expect("wallet requires an encryption secret");
        let wallet = Wallet::try_load(&self.store).await?;
        let cache = Cache::try_from((wallet, &secret))?;

        *self.cache.lock().unwrap() = cache;

        Ok(())
    }

    pub async fn store(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        let secret = ctx.wallet_secret().await.ok_or(Error::WalletSecretRequired)?;
        let wallet = Wallet::try_from((&*self.cache(), &secret))?;
        wallet.try_store(&self.store).await?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct LocalStore {
    inner: Arc<LocalStoreInner>,
}

impl LocalStore {
    pub fn try_new(folder: Option<&str>, name: &str) -> Result<Self> {
        Ok(Self { inner: Arc::new(LocalStoreInner::try_new(folder, name)?) })
    }
}

#[async_trait]
impl Interface for LocalStore {
    fn as_prv_key_data_store(&self) -> Arc<dyn PrvKeyDataStore> {
        self.inner.clone()
    }

    fn as_account_store(&self) -> Arc<dyn AccountStore> {
        self.inner.clone()
    }

    fn as_metadata_store(&self) -> Arc<dyn MetadataStore> {
        self.inner.clone()
    }

    fn as_transaction_record_store(&self) -> Arc<dyn TransactionRecordStore> {
        self.inner.clone()
    }

    async fn create(&self) -> Result<()> {
        Ok(())
    }

    async fn open(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        self.inner.reload(ctx).await?;
        Ok(())
    }

    async fn commit(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        self.inner.store(ctx).await?;

        Ok(())
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl PrvKeyDataStore for LocalStoreInner {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Result<Box<dyn Iterator<Item = Arc<PrvKeyDataInfo>>>> {
        Ok(Box::new(KeydataIterator::new(self, options)))
    }

    async fn load_key_info(&self, prv_key_data_id: &PrvKeyDataId) -> Result<Option<Arc<PrvKeyDataInfo>>> {
        Ok(self.cache().prv_key_data_info.map.get(prv_key_data_id).cloned())
    }

    async fn load_key_data(&self, ctx: &Arc<dyn AccessContextT>, prv_key_data_id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let wallet_secret = ctx.wallet_secret().await.ok_or(Error::WalletSecretRequired)?;
        let prv_key_data_map: Decrypted<PrvKeyDataMap> = self.cache().prv_key_data.decrypt(wallet_secret)?;
        Ok(prv_key_data_map.get(prv_key_data_id).cloned())
    }

    async fn store(&self, ctx: &Arc<dyn AccessContextT>, prv_key_data: PrvKeyData) -> Result<()> {
        let wallet_secret = ctx.wallet_secret().await.ok_or(Error::WalletSecretRequired)?;
        let mut prv_key_data_map: Decrypted<PrvKeyDataMap> = self.cache().prv_key_data.decrypt(wallet_secret.clone())?;
        prv_key_data_map.insert(prv_key_data.id, prv_key_data);
        self.cache().prv_key_data.replace(prv_key_data_map.encrypt(wallet_secret)?);
        Ok(())
    }

    async fn remove(&self, ctx: &Arc<dyn AccessContextT>, prv_key_data_id: &PrvKeyDataId) -> Result<()> {
        let wallet_secret = ctx.wallet_secret().await.ok_or(Error::WalletSecretRequired)?;
        let mut prv_key_data_map: Decrypted<PrvKeyDataMap> = self.cache().prv_key_data.decrypt(wallet_secret.clone())?;
        prv_key_data_map.remove(prv_key_data_id);
        self.cache().prv_key_data.replace(prv_key_data_map.encrypt(wallet_secret)?);
        Ok(())
    }
}

#[async_trait]
impl AccountStore for LocalStoreInner {
    async fn iter(
        self: Arc<Self>,
        prv_key_data_id_filter: Option<PrvKeyDataId>,
        options: IteratorOptions,
    ) -> Result<Box<dyn Iterator<Item = Arc<Account>>>> {
        Ok(Box::new(AccountIterator::new(self, prv_key_data_id_filter, options)))
    }

    async fn len(self: Arc<Self>, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<usize> {
        let len = match prv_key_data_id_filter {
            Some(filter) => self.cache().accounts.vec.iter().filter(|account| account.prv_key_data_id == filter).count(),
            None => self.cache().accounts.vec.len(),
        };

        Ok(len)
    }

    async fn load(&self, ids: &[AccountId]) -> Result<Vec<Arc<Account>>> {
        self.cache().accounts.load(ids)
    }

    async fn store(&self, accounts: &[&Account]) -> Result<()> {
        let mut cache = self.cache();
        cache.accounts.store(accounts)?;

        let (extend, remove) = accounts.iter().fold((vec![], vec![]), |mut acc, account| {
            if account.is_visible {
                acc.0.push((account.id, (**account).clone()));
            } else {
                acc.1.push(&account.id);
            }

            acc
        });

        cache.metadata.remove(&remove)?;
        cache.metadata.extend(&extend)?;

        Ok(())
    }

    async fn remove(&self, ids: &[&AccountId]) -> Result<()> {
        self.cache().accounts.remove(ids)?;
        Ok(())
    }
}

#[async_trait]
impl MetadataStore for LocalStoreInner {
    async fn iter(
        self: Arc<Self>,
        filter: Option<PrvKeyDataId>,
        options: IteratorOptions,
    ) -> Result<Box<dyn Iterator<Item = Arc<Metadata>>>> {
        Ok(Box::new(MetadataIterator::new(self, filter, options)))
    }

    async fn load(&self, ids: &[AccountId]) -> Result<Vec<Arc<Metadata>>> {
        Ok(self.cache().metadata.load(ids)?)
    }
}

#[async_trait]
impl TransactionRecordStore for LocalStoreInner {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Result<Box<dyn Iterator<Item = TransactionRecordId>>> {
        Ok(Box::new(TransactionRecordIterator::new(self, options)))
    }

    async fn load(&self, ids: &[TransactionRecordId]) -> Result<Vec<Arc<TransactionRecord>>> {
        self.cache().transaction_records.load(ids)
    }

    async fn store(&self, transaction_records: &[&TransactionRecord]) -> Result<()> {
        self.cache().transaction_records.store(transaction_records)
    }

    async fn remove(&self, ids: &[&TransactionRecordId]) -> Result<()> {
        self.cache().transaction_records.remove(ids)
    }
}
