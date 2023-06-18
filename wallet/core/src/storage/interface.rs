use crate::imports::*;
use crate::iterator::*;
use crate::result::Result;
use crate::secret::Secret;
use async_trait::async_trait;
use downcast::{downcast_sync, AnySync};

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
    async fn iter(self: Arc<Self>) -> Result<Box<dyn Iterator<Item = Arc<PrvKeyDataInfo>>>> {
        self.iter_with_options(IteratorOptions::default()).await
    }

    async fn iter_with_options(self: Arc<Self>, options: IteratorOptions) -> Result<Box<dyn Iterator<Item = Arc<PrvKeyDataInfo>>>>;
    async fn load_key_info(&self, id: &PrvKeyDataId) -> Result<Option<Arc<PrvKeyDataInfo>>>;
    async fn load_key_data(&self, ctx: &Arc<dyn AccessContextT>, id: &PrvKeyDataId) -> Result<Option<PrvKeyData>>;
    async fn store(&self, ctx: &Arc<dyn AccessContextT>, data: PrvKeyData) -> Result<()>;
    async fn remove(&self, ctx: &Arc<dyn AccessContextT>, id: &PrvKeyDataId) -> Result<()>;
}

#[async_trait]
pub trait AccountStore: Send + Sync {
    async fn iter(self: Arc<Self>, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<Box<dyn Iterator<Item = Arc<Account>>>> {
        self.iter_with_options(prv_key_data_id_filter, IteratorOptions::default()).await
    }

    async fn iter_with_options(
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
    async fn iter(self: Arc<Self>, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<Box<dyn Iterator<Item = Arc<Metadata>>>> {
        self.iter_with_options(prv_key_data_id_filter, IteratorOptions::default()).await
    }

    async fn iter_with_options(
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

pub struct CreateArgs {
    pub name: Option<String>,
    pub user_hint: Option<String>,
    pub overwrite_wallet: bool,
}

impl CreateArgs {
    pub fn new(name: Option<String>, user_hint: Option<String>, overwrite_wallet: bool) -> Self {
        Self { name, user_hint, overwrite_wallet }
    }
}

pub struct OpenArgs {
    pub name: Option<String>,
}

impl OpenArgs {
    pub fn new(name: Option<String>) -> Self {
        Self { name }
    }
}

#[async_trait]
pub trait Interface: Send + Sync + AnySync {
    // checks if the wallet storage is present
    async fn exists(&self, name: Option<&str>) -> Result<bool>;

    // initialize wallet storage
    async fn create(&self, ctx: &Arc<dyn AccessContextT>, args: CreateArgs) -> Result<()>;

    async fn is_open(&self) -> Result<bool>;

    // establish an open state (load wallet data cache, connect to the database etc.)
    async fn open(&self, ctx: &Arc<dyn AccessContextT>, args: OpenArgs) -> Result<()>;

    // flush writable operations (invoked after multiple store and remove operations)
    async fn commit(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()>;

    // stop the storage subsystem
    async fn close(&self) -> Result<()>;

    // ~~~

    async fn descriptor(&self) -> Result<Option<String>>;

    // ~~~

    // phishing hint (user-created text string identifying authenticity of the wallet)
    async fn get_user_hint(&self) -> Result<Option<Hint>>;
    async fn set_user_hint(&self, hint: Option<Hint>) -> Result<()>;

    // ~~~

    fn as_prv_key_data_store(&self) -> Result<Arc<dyn PrvKeyDataStore>>;
    fn as_account_store(&self) -> Result<Arc<dyn AccountStore>>;
    fn as_metadata_store(&self) -> Result<Arc<dyn MetadataStore>>;
    fn as_transaction_record_store(&self) -> Result<Arc<dyn TransactionRecordStore>>;
}

downcast_sync!(dyn Interface);
