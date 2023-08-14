use crate::imports::*;
use crate::result::Result;
use crate::storage::interface::AddressBookStore;
use crate::storage::interface::CreateArgs;
use crate::storage::interface::OpenArgs;
use crate::storage::interface::StorageStream;
use crate::storage::local::cache::*;
use crate::storage::local::streams::*;
use crate::storage::local::transaction::*;
use crate::storage::local::wallet::Wallet;
use crate::storage::local::Payload;
use crate::storage::local::Storage;
use crate::storage::*;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use workflow_core::runtime::is_web;

pub enum Store {
    Resident,
    Storage(Storage),
}

pub(crate) struct LocalStoreInner {
    pub cache: Arc<Mutex<Cache>>,
    pub store: Store,
    pub transactions: Arc<dyn TransactionRecordStore>,
    pub is_modified: AtomicBool,
    pub name: String,
}

impl LocalStoreInner {
    pub async fn try_create(ctx: &Arc<dyn AccessContextT>, folder: &str, args: CreateArgs, is_resident: bool) -> Result<Self> {
        let (store, name) = if is_resident {
            (Store::Resident, "resident".to_string())
        } else {
            // log_info!("LocalStoreInner::try_create: folder: {}, args: {:?}, is_resident: {}", folder, args, is_resident);

            let name = args.name.clone().unwrap_or(super::DEFAULT_WALLET_FILE.to_string());

            let storage = Storage::new_with_folder(folder, &format!("{name}.wallet"))?;
            if storage.exists().await? && !args.overwrite_wallet {
                return Err(Error::WalletAlreadyExists);
            }
            (Store::Storage(storage), name)
        };

        let secret = ctx.wallet_secret().await;
        let payload = Payload::default();
        let cache = Arc::new(Mutex::new(Cache::try_from((args.user_hint, payload, &secret))?));
        let modified = AtomicBool::new(false);
        let transactions: Arc<dyn TransactionRecordStore> = if !is_web() {
            Arc::new(fsio::TransactionStore::new(folder, &name))
        } else {
            Arc::new(indexdb::TransactionStore::new(&name))
        };

        Ok(Self { cache, store, is_modified: modified, name, transactions })
    }

    pub async fn try_load(ctx: &Arc<dyn AccessContextT>, folder: &str, args: OpenArgs) -> Result<Self> {
        let name = args.name.unwrap_or(super::DEFAULT_WALLET_FILE.to_string());
        let storage = Storage::new_with_folder(folder, &format!("{name}.wallet"))?;

        let secret = ctx.wallet_secret().await;
        let wallet = Wallet::try_load(&storage).await?;
        let cache = Arc::new(Mutex::new(Cache::try_from((wallet, &secret))?));
        let modified = AtomicBool::new(false);

        let transactions: Arc<dyn TransactionRecordStore> = if !is_web() {
            Arc::new(fsio::TransactionStore::new(folder, &name))
        } else {
            Arc::new(indexdb::TransactionStore::new(&name))
        };

        Ok(Self { cache, store: Store::Storage(storage), is_modified: modified, name, transactions })
    }

    pub fn cache(&self) -> MutexGuard<Cache> {
        self.cache.lock().unwrap()
    }

    // pub async fn reload(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()> {
    //     let secret = ctx.wallet_secret().await.expect("wallet requires an encryption secret");
    //     let wallet = Wallet::try_load(&self.store).await?;
    //     let cache = Cache::try_from((wallet, &secret))?;
    //     self.cache.lock().unwrap().replace(cache);
    //     Ok(())
    // }

    pub async fn store(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        match self.store {
            Store::Resident => Ok(()),
            Store::Storage(ref storage) => {
                let secret = ctx.wallet_secret().await; //.ok_or(Error::WalletSecretRequired)?;
                let wallet = Wallet::try_from((&*self.cache(), &secret))?;
                wallet.try_store(storage).await?;
                self.set_modified(false);
                Ok(())
            }
        }
    }

    #[inline]
    pub fn set_modified(&self, modified: bool) {
        match self.store {
            Store::Resident => (),
            Store::Storage(_) => {
                self.is_modified.store(modified, Ordering::SeqCst);
            }
        }
    }

    #[inline]
    pub fn is_modified(&self) -> bool {
        match self.store {
            Store::Resident => false,
            Store::Storage(_) => self.is_modified.load(Ordering::SeqCst),
        }
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

impl Drop for LocalStoreInner {
    fn drop(&mut self) {
        if self.is_modified() {
            panic!("LocalStoreInner::drop called while modified flag is true");
        }
    }
}

pub struct Location {
    pub folder: String,
}

impl Location {
    pub fn new(folder: &str) -> Self {
        Self { folder: folder.to_string() }
    }
}

impl Default for Location {
    fn default() -> Self {
        Self { folder: super::DEFAULT_STORAGE_FOLDER.to_string() }
    }
}

#[derive(Clone)]
pub(crate) struct LocalStore {
    location: Arc<Mutex<Option<Arc<Location>>>>,
    inner: Arc<Mutex<Option<Arc<LocalStoreInner>>>>,
    is_resident: bool,
    batch: Arc<AtomicBool>,
}

impl LocalStore {
    pub fn try_new(is_resident: bool) -> Result<Self> {
        Ok(Self {
            location: Arc::new(Mutex::new(Some(Arc::new(Location::default())))),
            inner: Arc::new(Mutex::new(None)),
            is_resident,
            batch: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn inner(&self) -> Result<Arc<LocalStoreInner>> {
        self.inner.lock().unwrap().as_ref().cloned().ok_or(Error::WalletNotOpen)
    }
}

#[async_trait]
impl Interface for LocalStore {
    fn as_prv_key_data_store(&self) -> Result<Arc<dyn PrvKeyDataStore>> {
        Ok(self.inner()?)
    }

    fn as_account_store(&self) -> Result<Arc<dyn AccountStore>> {
        Ok(self.inner()?)
    }

    fn as_address_book_store(&self) -> Result<Arc<dyn AddressBookStore>> {
        Ok(self.inner()?)
    }

    fn as_metadata_store(&self) -> Result<Arc<dyn MetadataStore>> {
        Ok(self.inner()?)
    }

    fn as_transaction_record_store(&self) -> Result<Arc<dyn TransactionRecordStore>> {
        Ok(self.inner()?.transactions.clone())
    }

    fn name(&self) -> Option<String> {
        self.inner.lock().unwrap().as_ref().map(|inner| inner.name.clone())
    }

    async fn exists(&self, name: Option<&str>) -> Result<bool> {
        let location = self.location.lock().unwrap().clone().unwrap();
        let store = Storage::new_with_folder(&location.folder, name.unwrap_or(super::DEFAULT_WALLET_FILE))?;
        store.exists().await
    }

    async fn create(&self, ctx: &Arc<dyn AccessContextT>, args: CreateArgs) -> Result<()> {
        let location = self.location.lock().unwrap().clone().unwrap();

        log_info!("WALLET CREATE INTERFACE: {:?}", args);
        let inner = Arc::new(LocalStoreInner::try_create(ctx, &location.folder, args, self.is_resident).await?);
        self.inner.lock().unwrap().replace(inner);

        Ok(())
    }

    async fn open(&self, ctx: &Arc<dyn AccessContextT>, args: OpenArgs) -> Result<()> {
        let location = self.location.lock().unwrap().clone().unwrap();
        let inner = Arc::new(LocalStoreInner::try_load(ctx, &location.folder, args).await?);
        self.inner.lock().unwrap().replace(inner);
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    fn descriptor(&self) -> Result<Option<String>> {
        let inner = self.inner()?;
        match inner.store {
            Store::Resident => Ok(Some("Memory resident wallet".to_string())),
            Store::Storage(ref storage) => Ok(Some(storage.filename_as_string())),
        }
    }

    async fn batch(&self) -> Result<()> {
        self.batch.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn flush(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        self.batch.store(false, Ordering::SeqCst);
        self.commit(ctx).await?;
        Ok(())
    }

    async fn commit(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()> {
        if !self.batch.load(Ordering::SeqCst) {
            // log_info!("*** COMMITING ***");
            self.inner()?.store(ctx).await?;
        } else {
            // log_info!("*** BATCH MODE - SKIPPING COMMIT ***");
        }
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        if self.inner()?.is_modified() {
            panic!("LocalStore::close called while modified flag is true");
        }

        if !self.is_open() {
            panic!("LocalStore::close called while wallet is not open");
        }

        let inner = self.inner.lock().unwrap().take().unwrap();
        inner.close().await?;

        Ok(())
    }

    async fn get_user_hint(&self) -> Result<Option<Hint>> {
        Ok(self.inner()?.cache().user_hint.clone())
    }

    async fn set_user_hint(&self, user_hint: Option<Hint>) -> Result<()> {
        self.inner()?.cache().user_hint = user_hint;
        Ok(())
    }
}

#[async_trait]
impl PrvKeyDataStore for LocalStoreInner {
    async fn iter(&self) -> Result<StorageStream<PrvKeyDataInfo>> {
        Ok(Box::pin(PrvKeyDataInfoStream::new(self.cache.clone())))
    }

    async fn load_key_info(&self, prv_key_data_id: &PrvKeyDataId) -> Result<Option<Arc<PrvKeyDataInfo>>> {
        Ok(self.cache().prv_key_data_info.map.get(prv_key_data_id).cloned())
    }

    async fn load_key_data(&self, ctx: &Arc<dyn AccessContextT>, prv_key_data_id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let wallet_secret = ctx.wallet_secret().await; //.ok_or(Error::WalletSecretRequired)?;
        let prv_key_data_map: Decrypted<PrvKeyDataMap> = self.cache().prv_key_data.decrypt(&wallet_secret)?;
        Ok(prv_key_data_map.get(prv_key_data_id).cloned())
    }

    async fn store(&self, ctx: &Arc<dyn AccessContextT>, prv_key_data: PrvKeyData) -> Result<()> {
        let wallet_secret = ctx.wallet_secret().await; //.ok_or(Error::WalletSecretRequired)?;
                                                       // log_info!("prv_key_data: {:?}", self.cache().prv_key_data);
        let mut prv_key_data_map: Decrypted<PrvKeyDataMap> = self.cache().prv_key_data.decrypt(&wallet_secret)?;
        prv_key_data_map.insert(prv_key_data.id, prv_key_data);
        self.cache().prv_key_data.replace(prv_key_data_map.encrypt(&wallet_secret)?);
        self.set_modified(true);
        Ok(())
    }

    async fn remove(&self, ctx: &Arc<dyn AccessContextT>, prv_key_data_id: &PrvKeyDataId) -> Result<()> {
        let wallet_secret = ctx.wallet_secret().await; //.ok_or(Error::WalletSecretRequired)?;
        let mut prv_key_data_map: Decrypted<PrvKeyDataMap> = self.cache().prv_key_data.decrypt(&wallet_secret)?;
        prv_key_data_map.remove(prv_key_data_id);
        self.cache().prv_key_data.replace(prv_key_data_map.encrypt(&wallet_secret)?);
        self.set_modified(true);
        Ok(())
    }
}

#[async_trait]
impl AccountStore for LocalStoreInner {
    async fn iter(&self, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<StorageStream<Account>> {
        Ok(Box::pin(AccountStream::new(self.cache.clone(), prv_key_data_id_filter)))
    }

    async fn len(&self, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<usize> {
        let len = match prv_key_data_id_filter {
            Some(filter) => self.cache().accounts.vec.iter().filter(|account| account.prv_key_data_id == filter).count(),
            None => self.cache().accounts.vec.len(),
        };

        Ok(len)
    }

    async fn load_single(&self, ids: &AccountId) -> Result<Option<Arc<Account>>> {
        self.cache().accounts.load_single(ids)
    }

    async fn load_multiple(&self, ids: &[AccountId]) -> Result<Vec<Arc<Account>>> {
        self.cache().accounts.load_multiple(ids)
    }

    async fn store(&self, accounts: &[&Account]) -> Result<()> {
        let mut cache = self.cache();
        cache.accounts.store(accounts)?;

        let (extend, remove) = accounts.iter().fold((vec![], vec![]), |mut acc, account| {
            if account.settings.is_visible {
                acc.0.push((account.id, (**account).clone()));
            } else {
                acc.1.push(&account.id);
            }

            acc
        });

        cache.metadata.remove(&remove)?;
        cache.metadata.extend(&extend)?;

        self.set_modified(true);

        Ok(())
    }

    async fn remove(&self, ids: &[&AccountId]) -> Result<()> {
        self.cache().accounts.remove(ids)?;

        self.set_modified(true);

        Ok(())
    }
}

#[async_trait]
impl AddressBookStore for LocalStoreInner {
    async fn iter(&self) -> Result<StorageStream<AddressBookEntry>> {
        Ok(Box::pin(AddressBookEntryStream::new(self.cache.clone())))
    }

    async fn search(&self, search: &str) -> Result<Vec<Arc<AddressBookEntry>>> {
        let matches = self
            .cache()
            .address_book
            .iter()
            .filter_map(|entry| if entry.alias.contains(search) { Some(Arc::new(entry.clone())) } else { None })
            .collect();

        Ok(matches)
    }
}

#[async_trait]
impl MetadataStore for LocalStoreInner {
    async fn iter(&self, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<StorageStream<Metadata>> {
        Ok(Box::pin(MetadataStream::new(self.cache.clone(), prv_key_data_id_filter)))
    }

    async fn load(&self, ids: &[AccountId]) -> Result<Vec<Arc<Metadata>>> {
        Ok(self.cache().metadata.load_multiple(ids)?)
    }
}
