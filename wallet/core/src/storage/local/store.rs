use crate::imports::*;
use crate::result::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
#[allow(unused_imports)]
use workflow_core::runtime;
use workflow_store::fs;

use super::collection::Collection;
use crate::storage::*;

/// Wallet file storage interface
#[wasm_bindgen(inspectable)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Store {
    filename: PathBuf,
}

#[wasm_bindgen]
impl Store {
    #[wasm_bindgen(getter, js_name = filename)]
    pub fn filename_as_string(&self) -> String {
        self.filename.to_str().unwrap().to_string()
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new(super::DEFAULT_WALLET_FOLDER, super::DEFAULT_WALLET_NAME).unwrap()
    }
}

impl Store {
    pub fn new(folder: &str, name: &str) -> Result<Store> {
        let filename = if runtime::is_web() {
            PathBuf::from(name) //filename.file_name().ok_or(Error::InvalidFilename(format!("{}", filename.display())))?)
        } else {
            // let filename = Path::new(DEFAULT_WALLET_FOLDER).join(name);
            let filename = Path::new(folder).join(name);
            let filename = fs::resolve_path(filename.to_str().unwrap());
            filename
        };

        Ok(Store { filename })
    }

    pub fn filename(&self) -> &PathBuf {
        &self.filename
    }

    pub async fn purge(&self) -> Result<()> {
        workflow_store::fs::remove(self.filename()).await?;
        Ok(())
    }

    pub async fn exists(&self) -> Result<bool> {
        Ok(workflow_store::fs::exists(self.filename()).await?)
    }

    pub async fn ensure_dir(&self) -> Result<()> {
        let file = self.filename();
        if file.exists() {
            return Ok(());
        }

        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir).await?;
        }
        Ok(())
    }
}

// pub struct Settings;

#[derive(Default)]
pub struct LocalStoreCache {
    prv_key_data: Mutex<Collection<PrvKeyDataId, PrvKeyData>>,
    accounts: Mutex<Collection<AccountId, Account>>,
    metadata: Mutex<Collection<AccountId, Metadata>>,
    transaction_records: Mutex<Collection<TransactionRecordId, TransactionRecord>>,
}

pub struct LocalStore {
    pub wallet: Store,
    pub cache: LocalStoreCache,
}

impl LocalStore {
    pub fn new(_folder: &Path, name: &str) -> Result<LocalStore> {
        let wallet = Store::new(super::DEFAULT_WALLET_FOLDER, name)?;
        // let transactions = Store::new(name)?;

        Ok(LocalStore { wallet, cache: LocalStoreCache::default() })
    }
}

#[derive(Clone)]
struct StoreIteratorInner {
    store: Arc<LocalStore>,
    cursor: usize,
    chunk_size: usize,
}

impl std::fmt::Debug for StoreIteratorInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreIteratorInner").field("cursor", &self.cursor).finish()
    }
}

static DEFAULT_CHUNK_SIZE: usize = 25;
#[macro_export]
macro_rules! declare_iterator {
    ($prop:ident, $name:ident, $id : ty, $data : ty) => {
        #[derive(Clone, Debug)]
        pub struct $name(StoreIteratorInner);

        impl $name {
            pub fn new(store: Arc<LocalStore>, iterator_options: IteratorOptions) -> Self {
                Self(StoreIteratorInner { store, cursor: 0, chunk_size: iterator_options.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE) })
            }
        }

        #[async_trait]
        impl Iterator for $name {
            type Item = $id;

            async fn next(&mut self) -> Option<Vec<Self::Item>> {
                let vec = &self.0.store.cache.$prop.lock().unwrap().vec;
                if self.0.cursor >= vec.len() {
                    return None;
                } else {
                    let slice = &vec[self.0.cursor as usize..(self.0.cursor as usize + self.0.chunk_size)];
                    if slice.is_empty() {
                        None
                    } else {
                        self.0.cursor += slice.len();
                        let vec = slice.iter().map(|data| data.id.clone()).collect();
                        Some(vec)
                    }
                }
            }
        }
    };
}

declare_iterator!(prv_key_data, LocalStorePrvKeyDataIterator, PrvKeyDataId, PrvKeyData);
declare_iterator!(accounts, LocalStoreAccountIterator, AccountId, Account);
declare_iterator!(metadata, LocalStoreMetadataIterator, AccountId, Metadata);
declare_iterator!(transaction_records, LocalStoreTransactionRecordIterator, TransactionRecordId, TransactionRecord);

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
}

#[async_trait]
impl PrvKeyDataStore for LocalStore {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = PrvKeyDataId>> {
        Box::new(LocalStorePrvKeyDataIterator::new(self, options))
    }

    async fn store(&self, _ctx: &Arc<dyn AccessContextT>, _data: &[&PrvKeyData]) -> Result<()> {
        todo!();
    }

    async fn load(&self, _ctx: &Arc<dyn AccessContextT>, _id: &[PrvKeyDataId]) -> Result<Vec<PrvKeyData>> {
        todo!();
    }
}

#[async_trait]
impl AccountStore for LocalStore {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = AccountId>> {
        Box::new(LocalStoreAccountIterator::new(self, options))
    }

    async fn store(&self, _ctx: &Arc<dyn AccessContextT>, data: &[&Account]) -> Result<()> {
        self.cache.accounts.lock().unwrap().store(data)?;
        Ok(())
    }

    async fn load(&self, _ctx: &Arc<dyn AccessContextT>, _id: &[AccountId]) -> Result<Vec<Account>> {
        todo!();
    }
}

#[async_trait]
impl MetadataStore for LocalStore {
    // async fn iter(self: Arc<Self>, options : IteratorOptions) -> Arc<dyn Iterator<Item = AccountId>> {
    //     let iter = LocalStoreMetadataIterator::new(self.clone(), options);
    //     Arc::new(iter)
    // }

    async fn store(&self, _ctx: &Arc<dyn AccessContextT>, data: &[&Metadata]) -> Result<()> {
        self.cache.metadata.lock().unwrap().store(data)?;
        Ok(())
        // todo!();d
    }

    async fn load(&self, _ctx: &Arc<dyn AccessContextT>, _id: &[AccountId]) -> Result<Vec<Metadata>> {
        todo!();
    }
}

#[async_trait]
impl TransactionRecordStore for LocalStore {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = TransactionRecordId>> {
        Box::new(LocalStoreTransactionRecordIterator::new(self, options))
    }

    async fn store(&self, _ctx: &Arc<dyn AccessContextT>, _data: &[&TransactionRecord]) -> Result<()> {
        todo!();
    }

    async fn load(&self, _ctx: &Arc<dyn AccessContextT>, _id: &[TransactionRecordId]) -> Result<Vec<TransactionRecord>> {
        todo!();
    }
}
