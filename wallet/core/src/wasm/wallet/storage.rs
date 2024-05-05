
use std::fmt::Debug;
use std::fmt::Formatter;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use web_sys::Element;

// TODO - finish WASM storage implementation bindings
/// @alpha
#[wasm_bindgen]
extern "C" {

    #[wasm_bindgen(js_name="Storage")]
    pub type Storage;

    #[wasm_bindgen(method, js_name = "exists")]
    async fn exists(&self, name: Option<&str>) -> Result<bool>;

    // initialize wallet storage
    #[wasm_bindgen(method, js_name = "create")]
    async fn create(this: &Storage, wallet_secret: &Secret, args: CreateArgs) -> Result<()>;

    // async fn is_open(&self) -> Result<bool>;

    // establish an open state (load wallet data cache, connect to the database etc.)
    #[wasm_bindgen(method, js_name = "open")]
    async fn open(this: &Storage, wallet_secret: &Secret, args: OpenArgs) -> Result<()>;

    // flush writable operations (invoked after multiple store and remove operations)
    #[wasm_bindgen(method, js_name = "commit")]
    async fn commit(this: &Storage, wallet_secret: &Secret) -> Result<()>;

    // stop the storage subsystem
    #[wasm_bindgen(method, js_name = "close")]
    async fn close(this: &Storage) -> Result<()>;

    // return storage information string (file location)
    // #[wasm_bindgen(method, js_name = "descriptor")]
    // async fn descriptor(&self) -> Result<Option<String>>;

    // ~~~

    // phishing hint (user-created text string identifying authenticity of the wallet)
    // async fn get_user_hint(&self) -> Result<Option<Hint>>;
    // async fn set_user_hint(&self, hint: Option<Hint>) -> Result<()>;

    // ~~~


    #[wasm_bindgen(method, js_name = "getKeyInfoRange")]
    async fn get_key_info_range(this: &Storage, start: usize, stop : usize) -> Result<PrvKeyDataInfo>;
    #[wasm_bindgen(method, js_name = "loadKeyInfo")]
    async fn load_key_info(this: &Storage, id: &PrvKeyDataId) -> Result<Option<Arc<PrvKeyDataInfo>>>;
    #[wasm_bindgen(method, js_name = "loadKeyData")]
    async fn load_key_data(this: &Storage, wallet_secret: &Secret, id: &PrvKeyDataId) -> Result<Option<PrvKeyData>>;
    #[wasm_bindgen(method, js_name = "storeKeyInfo")]
    async fn store_key_info(this: &Storage, wallet_secret: &Secret, data: PrvKeyData) -> Result<()>;
    #[wasm_bindgen(method, js_name = "storeKeyData")]
    async fn store_key_data(this: &Storage, wallet_secret: &Secret, data: PrvKeyData) -> Result<()>;
    #[wasm_bindgen(method, js_name = "removeKeyData")]
    async fn remove_key_data(this: &Storage, wallet_secret: &Secret, id: &PrvKeyDataId) -> Result<()>;
    
    #[wasm_bindgen(method, js_name = "getAccountRange")]
    async fn get_account_range(this: &Storage, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<StorageStream<Account>>;
    #[wasm_bindgen(method, js_name = "getAccountLen")]
    async fn get_account_count(this: &Storage, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<usize>;
    #[wasm_bindgen(method, js_name = "loadAccounts")]
    async fn load_accounts(this: &Storage, ids: &[AccountId]) -> Result<Vec<Arc<Account>>>;
    #[wasm_bindgen(method, js_name = "storeAccounts")]
    async fn store_accounts(this: &Storage, data: &[&Account]) -> Result<()>;
    #[wasm_bindgen(method, js_name = "removeAccounts")]
    async fn remove_accounts(this: &Storage, id: &[&AccountId]) -> Result<()>;
    
    // pub trait MetadataStore: Send + Sync {
        // async fn get_metadata_range(&self, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<StorageStream<Metadata>>;
        // async fn load_metadata(&self, id: &[AccountId]) -> Result<Vec<Arc<Metadata>>>;
        
    #[wasm_bindgen(method, js_name = "getTransactionRecordRange")]
    async fn get_transaction_record_range(this: &Storage) -> Result<StorageStream<TransactionRecord>>;
    #[wasm_bindgen(method, js_name = "loadTransactionRecords")]
    async fn load_transaction_records(this: &Storage, id: &[TransactionRecordId]) -> Result<Vec<Arc<TransactionRecord>>>;
    #[wasm_bindgen(method, js_name = "storeTransactionRecords")]
    async fn store_transaction_records(this: &Storage, data: &[&TransactionRecord]) -> Result<()>;
    #[wasm_bindgen(method, js_name = "removeTransactionRecords")]
    async fn remove_transaction_records(this: &Storage, id: &[&TransactionRecordId]) -> Result<()>;

}


pub(crate) struct Inner {
    storage: Storage,
}

#[derive(Clone)]
pub(crate) struct Proxy {
    inner: Arc<Inner>,
}

impl Proxy {
    pub fn try_new(storage: Storage) -> Result<Self> {
        Ok(Self{ inner : Inner { storage : Arc::new(storage) }, })
    }
}

#[async_trait]
impl Interface for Proxy {
    fn as_prv_key_data_store(&self) -> Result<Arc<dyn PrvKeyDataStore>> {
        Ok(self.inner)
    }

    fn as_account_store(&self) -> Result<Arc<dyn AccountStore>> {
        Ok(self.inner)
    }

    fn as_metadata_store(&self) -> Result<Arc<dyn MetadataStore>> {
        Ok(self.inner)
    }

    fn as_transaction_record_store(&self) -> Result<Arc<dyn TransactionRecordStore>> {
        Ok(self.inner)
    }

    async fn exists(&self, name: Option<&str>) -> Result<bool> {
        let location = self.location.lock().unwrap().clone().unwrap();
        let store = Store::new(&location.folder, name.unwrap_or(super::DEFAULT_WALLET_FILE))?;
        store.exists().await
    }

    async fn create(&self, wallet_secret: &Secret, args: CreateArgs) -> Result<()> {
        let location = self.location.lock().unwrap().clone().unwrap();
        let inner = Arc::new(Inner::try_create(ctx, &location.folder, args).await?);
        self.inner.lock().unwrap().replace(inner);

        Ok(())
    }

    async fn open(&self, wallet_secret: &Secret, args: OpenArgs) -> Result<()> {
        let location = self.location.lock().unwrap().clone().unwrap();
        let inner = Arc::new(Inner::try_load(ctx, &location.folder, args).await?);
        self.inner.lock().unwrap().replace(inner);
        Ok(())
    }

    async fn is_open(&self) -> Result<bool> {
        Ok(self.inner.lock().unwrap().is_some())
    }

    async fn descriptor(&self) -> Result<Option<String>> {
        Ok(Some(self.inner()?.store.filename_as_string()))
    }

    async fn commit(&self, wallet_secret: &Secret) -> Result<()> {
        // log_info!("--== committing storage ==--");
        self.inner()?.store(ctx).await?;
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        if self.inner()?.is_modified() {
            panic!("Proxy::close called while modified flag is true");
        }

        if !self.is_open().await? {
            panic!("Proxy::close called while wallet is not open");
        }

        self.inner.lock().unwrap().take();

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
impl PrvKeyDataStore for Inner {
    async fn iter(&self) -> Result<StorageStream<PrvKeyDataInfo>> {
        Ok(Box::pin(PrvKeyDataInfoStream::new(self.cache.clone())))
    }

    async fn load_key_info(&self, prv_key_data_id: &PrvKeyDataId) -> Result<Option<Arc<PrvKeyDataInfo>>> {
        Ok(self.cache().prv_key_data_info.map.get(prv_key_data_id).cloned())
    }

    async fn load_key_data(&self, wallet_secret: &Secret, prv_key_data_id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let wallet_secret = ctx.wallet_secret().await;
        let prv_key_data_map: Decrypted<PrvKeyDataMap> = self.cache().prv_key_data.decrypt(wallet_secret)?;
        Ok(prv_key_data_map.get(prv_key_data_id).cloned())
    }

    async fn store(&self, wallet_secret: &Secret, prv_key_data: PrvKeyData) -> Result<()> {
        let wallet_secret = ctx.wallet_secret().await;
        let prv_key_data_info = Arc::new((&prv_key_data).into());
        self.cache().prv_key_data_info.insert(prv_key_data.id, prv_key_data_info)?;
        let mut prv_key_data_map: Decrypted<PrvKeyDataMap> = self.cache().prv_key_data.decrypt(wallet_secret.clone())?;
        prv_key_data_map.insert(prv_key_data.id, prv_key_data);
        self.cache().prv_key_data.replace(prv_key_data_map.encrypt(wallet_secret)?);
        self.set_modified(true);
        Ok(())
    }

    async fn remove(&self, wallet_secret: &Secret, prv_key_data_id: &PrvKeyDataId) -> Result<()> {
        let wallet_secret = ctx.wallet_secret().await;
        let mut prv_key_data_map: Decrypted<PrvKeyDataMap> = self.cache().prv_key_data.decrypt(wallet_secret.clone())?;
        prv_key_data_map.remove(prv_key_data_id);
        self.cache().prv_key_data.replace(prv_key_data_map.encrypt(wallet_secret)?);
        self.set_modified(true);
        Ok(())
    }
}

#[async_trait]
impl AccountStore for Inner {
    async fn iter(&self, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<StorageStream<Account>> {
        Ok(Box::pin(AccountStream::new(self.cache.clone(), prv_key_data_id_filter)))
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
impl MetadataStore for Inner {
    async fn iter(&self, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<StorageStream<Metadata>> {
        Ok(Box::pin(MetadataStream::new(self.cache.clone(), prv_key_data_id_filter)))
    }

    async fn load(&self, ids: &[AccountId]) -> Result<Vec<Arc<Metadata>>> {
        Ok(self.cache().metadata.load(ids)?)
    }
}

#[async_trait]
impl TransactionRecordStore for Inner {
    async fn iter(&self) -> Result<StorageStream<TransactionRecord>> {
        Ok(Box::pin(TransactionRecordStream::new(self.cache.clone())))
    }

    async fn load(&self, ids: &[TransactionRecordId]) -> Result<Vec<Arc<TransactionRecord>>> {
        self.cache().transaction_records.load(ids)
    }

    async fn store(&self, transaction_records: &[&TransactionRecord]) -> Result<()> {
        self.cache().transaction_records.store(transaction_records)?;
        self.set_modified(true);
        Ok(())
    }

    async fn remove(&self, ids: &[&TransactionRecordId]) -> Result<()> {
        self.cache().transaction_records.remove(ids)?;
        self.set_modified(true);
        Ok(())
    }
}
