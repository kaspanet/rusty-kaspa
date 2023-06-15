use crate::imports::*;
use crate::iterator::*;
use crate::result::Result;
use crate::secret::Secret;
use async_trait::async_trait;

use crate::storage::*;

/// AccessContextT is a trait that wraps a wallet secret
/// (or possibly other parameters in the future)
/// needed for accessing stored wallet data.
#[async_trait]
pub trait AccessContextT: Send + Sync {
    async fn wallet_secret(&self) -> Option<Secret>;
}

/// AccessContext is a wrapper for wallet secret that implements
/// the [`AccessContextT`] trait.
#[derive(Clone, Default)]
pub struct AccessContext {
    pub(crate) wallet_secret: Option<Secret>,
}

impl AccessContext {
    pub fn new(wallet_secret: Option<Secret>) -> Self {
        Self { wallet_secret }
    }
}

#[async_trait]
impl AccessContextT for AccessContext {
    async fn wallet_secret(&self) -> Option<Secret> {
        self.wallet_secret.clone()
    }
}

#[async_trait]
pub trait PrvKeyDataStore: Send + Sync {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Result<Box<dyn Iterator<Item = Arc<PrvKeyDataInfo>>>>;
    async fn load_key_info(&self, id: &PrvKeyDataId) -> Result<Option<Arc<PrvKeyDataInfo>>>;
    async fn load_key_data(&self, ctx: &Arc<dyn AccessContextT>, id: &PrvKeyDataId) -> Result<Option<PrvKeyData>>;
    async fn store(&self, ctx: &Arc<dyn AccessContextT>, data: PrvKeyData) -> Result<()>;
    async fn remove(&self, ctx: &Arc<dyn AccessContextT>, id: &PrvKeyDataId) -> Result<()>;
}

#[async_trait]
pub trait AccountStore: Send + Sync {
    async fn iter(
        self: Arc<Self>,
        prv_key_data_id_filter: Option<PrvKeyDataId>,
        options: IteratorOptions,
    ) -> Result<Box<dyn Iterator<Item = Arc<Account>>>>;
    async fn len(self: Arc<Self>, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<usize>;
    async fn load(&self, ids: &[AccountId]) -> Result<Vec<Arc<Account>>>;
    async fn store(&self, data: &[&Account]) -> Result<()>;
    async fn remove(&self, id: &[&AccountId]) -> Result<()>;
}

#[async_trait]
pub trait MetadataStore: Send + Sync {
    async fn iter(
        self: Arc<Self>,
        prv_key_data_id_filter: Option<PrvKeyDataId>,
        options: IteratorOptions,
    ) -> Result<Box<dyn Iterator<Item = Arc<Metadata>>>>;

    async fn load(&self, id: &[AccountId]) -> Result<Vec<Arc<Metadata>>>;
}

#[async_trait]
pub trait TransactionRecordStore: Send + Sync {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Result<Box<dyn Iterator<Item = TransactionRecordId>>>;
    async fn load(&self, id: &[TransactionRecordId]) -> Result<Vec<Arc<TransactionRecord>>>;
    async fn store(&self, data: &[&TransactionRecord]) -> Result<()>;
    async fn remove(&self, id: &[&TransactionRecordId]) -> Result<()>;
}

#[async_trait]
// pub trait Interface: Sized + Send + Sync {
pub trait Interface: Send + Sync {

    // initialize wallet storage
    async fn create(&self) -> Result<()>;
    // establish an open state (load wallet data cache, connect to the database etc.)
    async fn open(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()>;
    // flush writable operations (invoked after multiple store and remove operations)
    async fn commit(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()>;
    // stop the storage subsystem
    async fn close(&self) -> Result<()>;

    // ~~~

    fn as_prv_key_data_store(&self) -> Arc<dyn PrvKeyDataStore>;
    fn as_account_store(&self) -> Arc<dyn AccountStore>;
    fn as_metadata_store(&self) -> Arc<dyn MetadataStore>;
    fn as_transaction_record_store(&self) -> Arc<dyn TransactionRecordStore>;

    // ------
    // - an alternative flat (no traits) structure example:
    //
    // async fn prv_key_data_iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = Arc<PrvKeyDataInfo>>>;
    // async fn prv_key_data_load(&self, id: &PrvKeyDataId) -> Result<Option<PrvKeyData>>;
    // async fn prv_key_data_store(&self, ctx: &Arc<dyn AccessContextT>, data: &[&PrvKeyData]) -> Result<()>;
    // async fn prv_key_data_remove(&self, ctx: &Arc<dyn AccessContextT>, id: &[&PrvKeyDataId]) -> Result<()>;
    // ---
    // async fn account_iter(
    //     self: Arc<Self>,
    //     prv_key_data_id_filter: Option<PrvKeyDataId>,
    //     options: IteratorOptions,
    // ) -> Box<dyn Iterator<Item = Arc<Account>>>;
    // async fn account_len(self : Arc<Self>, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<usize>;
    // async fn account_load(&self, ids: &[AccountId]) -> Result<Vec<Arc<Account>>>;
    // async fn account_store(&self, ctx: &Arc<dyn AccessContextT>, data: &[&Account]) -> Result<()>;
    // async fn account_remove(&self, ctx: &Arc<dyn AccessContextT>, id: &[AccountId]) -> Result<()>;
    // ---
    // async fn metadata_iter(
    //     self: Arc<Self>,
    //     prv_key_data_id_filter: Option<PrvKeyDataId>,
    //     options: IteratorOptions,
    // ) -> Box<dyn Iterator<Item = Arc<Metadata>>>;
    //
    // async fn metadata_load(&self, id: &[AccountId]) -> Result<Vec<Metadata>>;
    // ---
    // async fn transaction_record_iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = TransactionRecordId>>;
    // async fn transaction_record_load(&self, id: &[&TransactionRecordId]) -> Result<Vec<TransactionRecord>>;
    // async fn transaction_record_store(&self, ctx: &Arc<dyn AccessContextT>, data: &[&TransactionRecord]) -> Result<()>;
    // async fn transaction_record_remove(&self, ctx: &Arc<dyn AccessContextT>, id: &[&TransactionRecordId]) -> Result<()>;
    // ---

}
