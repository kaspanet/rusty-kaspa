use std::collections::HashMap;

use crate::imports::*;
use crate::iterator::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::local::iterators::*;
use crate::storage::local::*;
use crate::storage::*;
use async_trait::async_trait;

#[derive(Default)]
pub struct Cache {
    pub prv_key_data: Encrypted, //Mutex<Collection<PrvKeyDataId, PrvKeyData>>,
    // pub prv_key_data_ids: Vec<PrvKeyDataId>,
    pub prv_key_data_info: Vec<Arc<PrvKeyDataInfo>>,
    pub accounts: Collection<AccountId, Account>,
    pub metadata: Collection<AccountId, Metadata>,
    pub transaction_records: Collection<TransactionRecordId, TransactionRecord>,
}

impl TryFrom<(Wallet, &Secret)> for Cache {
    type Error = Error;
    fn try_from((wallet, secret): (Wallet, &Secret)) -> Result<Self> {
        let payload = wallet.payload(secret.clone())?;

        let prv_key_data_info = payload.0.prv_key_data.iter().map(|pkdata| Arc::new(pkdata.into())).collect();
        let prv_key_data_map = payload.0.prv_key_data.into_iter().map(|pkdata| (pkdata.id, pkdata)).collect::<HashMap<_, _>>();
        let prv_key_data: Decrypted<PrvKeyDataMap> = Decrypted::new(prv_key_data_map);
        let prv_key_data = prv_key_data.encrypt(secret.clone())?;
        let accounts: Collection<AccountId, Account> = payload.0.accounts.try_into()?;
        let metadata: Collection<AccountId, Metadata> = wallet.metadata.try_into()?;
        // let transaction_records : Collection<TransactionRecordId,TransactionRecord> = wallet.transaction_records.try_into()?;
        let transaction_records: Collection<TransactionRecordId, TransactionRecord> = Collection::default();

        Ok(Cache { prv_key_data, prv_key_data_info, accounts, metadata, transaction_records })
    }
}

impl TryFrom<(&Cache, &Secret)> for Wallet {
    type Error = Error;

    fn try_from((cache, secret): (&Cache, &Secret)) -> Result<Self> {
        // let inner = cache.inner();
        // let prv_key_data: Vec<PrvKeyData> = cache.prv_key_data.decrypt::<PrvKeyDataMap>(secret.clone())?.decrypt()?;
        let prv_key_data: Vec<PrvKeyData> = vec![];
        let accounts: Vec<Account> = (&cache.accounts).try_into()?;
        let metadata: Vec<Metadata> = (&cache.metadata).try_into()?;

        let payload = Payload { prv_key_data, accounts };

        let payload = Decrypted::new(payload).encrypt(secret.clone())?;

        Ok(Wallet { payload, metadata })
    }
}

pub struct LocalStoreCache {
    pub inner: Mutex<Cache>,
    pub store: Store,
}

impl LocalStoreCache {
    pub fn try_new(folder: Option<&str>, name: &str) -> Result<LocalStoreCache> {
        let store = Store::new(folder.unwrap_or(super::DEFAULT_WALLET_FOLDER), name)?;
        Ok(Self { inner: Mutex::new(Cache::default()), store })
    }

    pub fn inner(&self) -> MutexGuard<Cache> {
        self.inner.lock().unwrap()
    }

    // - TODO - INIT ???  RELOAD(SOME(ctx)) ???

    pub async fn reload(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        let secret = ctx.wallet_secret().await.expect("wallet requires an encryption secret");
        let wallet = Wallet::try_load(&self.store).await?;
        let cache = Cache::try_from((wallet, &secret))?;

        *self.inner.lock().unwrap() = cache;

        Ok(())
    }
}

pub struct LocalStore {
    pub cache: LocalStoreCache,
}

impl LocalStore {
    // pub fn new(folder: &Path, name: &str) -> Result<LocalStore> {
    pub fn try_new(folder: Option<&str>, name: &str) -> Result<LocalStore> {
        Ok(LocalStore { cache: LocalStoreCache::try_new(folder, name)? })
    }
}

#[async_trait]
impl Interface for LocalStore {
    async fn prv_key_data(self: Arc<Self>) -> Arc<dyn PrvKeyDataStore> {
        self
    }
    async fn account(self: Arc<Self>) -> Arc<dyn AccountStore> {
        self
    }
    async fn metadata(self: Arc<Self>) -> Arc<dyn MetadataStore> {
        self
    }
    async fn transaction_record(self: Arc<Self>) -> Arc<dyn TransactionRecordStore> {
        self
    }

    async fn open(&self, _ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        Ok(())
    }

    async fn close(&self, _ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl PrvKeyDataStore for LocalStore {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = Arc<PrvKeyDataInfo>>> {
        // todo!()
        Box::new(PrvKeyDataIterator::new(self, options))
    }

    async fn len(&self, _ctx: &Arc<dyn AccessContextT>) -> Result<usize> {
        Ok(self.cache.inner().prv_key_data_info.len())
    }

    async fn store(&self, _ctx: &Arc<dyn AccessContextT>, _data: &[&PrvKeyData]) -> Result<()> {
        todo!();
    }

    async fn load(&self, _ctx: &Arc<dyn AccessContextT>, _id: &[&PrvKeyDataId]) -> Result<Vec<PrvKeyData>> {
        todo!();
    }

    // async fn range(&self, _ctx: &Arc<dyn AccessContextT>, range : std::ops::Range<usize>) -> Result<Vec<PrvKeyDataInfo>> {

    //     let accounts = self.cache.inner().prv_key_data_info[range.start..range.end].to_vec(); //accounts.range(range)?;
    //     Ok(accounts)

    //     // todo!();

    // }

    async fn remove(&self, _ctx: &Arc<dyn AccessContextT>, _id: &[&PrvKeyDataId]) -> Result<()> {
        todo!();
    }
}

#[async_trait]
impl AccountStore for LocalStore {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = AccountId>> {
        Box::new(AccountIterator::new(self, options))
    }

    async fn len(&self, _ctx: &Arc<dyn AccessContextT>) -> Result<usize> {
        Ok(self.cache.inner().accounts.vec.len())
    }

    async fn store(&self, _ctx: &Arc<dyn AccessContextT>, data: &[&Account]) -> Result<()> {
        // self.cache.accounts.lock().unwrap().store(data)?;
        // let secret = ctx.wallet_secret().await.expect("wallet requires an encryption secret");
        self.cache.inner().accounts.store(data)?;
        // let accounts = accounts.into_iter().map(|account| *account.clone()).collect::<Vec<_>>();
        // Ok(accounts)
        Ok(())
    }

    async fn load(&self, _ctx: &Arc<dyn AccessContextT>, ids: &[AccountId]) -> Result<Vec<Arc<Account>>> {
        // self.cache.reload(ctx).await?;
        // let secret = ctx.wallet_secret().await.expect("wallet requires an encryption secret");
        let accounts = self.cache.inner().accounts.load(ids)?;
        // let accounts = accounts.into_iter().map(|account| (*account).clone()).collect::<Vec<_>>();
        Ok(accounts)
    }

    async fn range(&self, _ctx: &Arc<dyn AccessContextT>, range: std::ops::Range<usize>) -> Result<Vec<Arc<Account>>> {
        let accounts = self.cache.inner().accounts.range(range)?;
        Ok(accounts)
    }

    async fn remove(&self, _ctx: &Arc<dyn AccessContextT>, ids: &[AccountId]) -> Result<()> {
        self.cache.inner().accounts.remove(ids)?;
        Ok(())
        // todo!();
    }
}

#[async_trait]
impl MetadataStore for LocalStore {
    // async fn iter(self: Arc<Self>, options : IteratorOptions) -> Arc<dyn Iterator<Item = AccountId>> {
    //     let iter = LocalStoreMetadataIterator::new(self.clone(), options);
    //     Arc::new(iter)
    // }

    async fn store(&self, _ctx: &Arc<dyn AccessContextT>, data: &[&Metadata]) -> Result<()> {
        // self.cache.metadata.lock().unwrap().store(data)?;
        self.cache.inner().metadata.store(data)?;

        Ok(())
        // todo!();d
    }

    async fn load(&self, _ctx: &Arc<dyn AccessContextT>, ids: &[AccountId]) -> Result<Vec<Metadata>> {
        let metadata = self.cache.inner().metadata.load(ids)?;
        let metadata = metadata.into_iter().map(|metadata| (*metadata).clone()).collect::<Vec<_>>();
        Ok(metadata)

        // todo!();
    }

    async fn remove(&self, _ctx: &Arc<dyn AccessContextT>, ids: &[AccountId]) -> Result<()> {
        // todo!();
        self.cache.inner().accounts.remove(ids)?;
        Ok(())
    }
}

#[async_trait]
impl TransactionRecordStore for LocalStore {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = TransactionRecordId>> {
        Box::new(TransactionRecordIterator::new(self, options))
    }

    async fn store(&self, _ctx: &Arc<dyn AccessContextT>, _data: &[&TransactionRecord]) -> Result<()> {
        todo!();
    }

    async fn load(&self, _ctx: &Arc<dyn AccessContextT>, _id: &[&TransactionRecordId]) -> Result<Vec<TransactionRecord>> {
        todo!();
    }

    async fn remove(&self, _ctx: &Arc<dyn AccessContextT>, _id: &[&TransactionRecordId]) -> Result<()> {
        todo!();
    }
}
