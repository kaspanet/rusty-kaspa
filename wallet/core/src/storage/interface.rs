use crate::imports::*;
use crate::iterator::*;
use crate::result::Result;
use crate::secret::Secret;
use async_trait::async_trait;

use crate::storage::*;

#[async_trait]
pub trait AccessContextT: Send + Sync {
    async fn wallet_secret(&self) -> Option<Secret>;
    async fn payment_secret(&self, account: &Arc<Account>) -> Option<Secret>;
}

#[derive(Clone, Default)]
pub struct AccessContext {
    pub(crate) wallet_secret: Option<Secret>,
    pub(crate) payment_secret: Option<Secret>,
}

impl AccessContext {
    pub fn new_with_wallet_secret(wallet_secret: Secret) -> Self {
        Self { wallet_secret: Some(wallet_secret), payment_secret: None }
    }

    pub fn new_with_args(wallet_secret: Option<Secret>, payment_secret: Option<Secret>) -> Self {
        Self { wallet_secret, payment_secret }
    }
}

#[async_trait]
impl AccessContextT for AccessContext {
    async fn wallet_secret(&self) -> Option<Secret> {
        self.wallet_secret.clone()
    }
    async fn payment_secret(&self, _account: &Arc<Account>) -> Option<Secret> {
        self.payment_secret.clone()
    }
}

#[async_trait]
pub trait PrvKeyDataStore: Send + Sync {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = Arc<PrvKeyDataInfo>>>;
    async fn len(&self, ctx: &Arc<dyn AccessContextT>) -> Result<usize>;
    async fn load(&self, ctx: &Arc<dyn AccessContextT>, id: &[&PrvKeyDataId]) -> Result<Vec<PrvKeyData>>;
    // async fn range(&self, ctx: &Arc<dyn AccessContextT>, range : std::ops::Range<usize>) -> Result<Vec<Arc<PrvKeyData>>>;
    async fn store(&self, ctx: &Arc<dyn AccessContextT>, data: &[&PrvKeyData]) -> Result<()>;
    async fn remove(&self, ctx: &Arc<dyn AccessContextT>, id: &[&PrvKeyDataId]) -> Result<()>;
}

#[async_trait]
pub trait AccountStore: Send + Sync {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = AccountId>>;
    async fn len(&self, ctx: &Arc<dyn AccessContextT>) -> Result<usize>;
    async fn load(&self, ctx: &Arc<dyn AccessContextT>, ids: &[AccountId]) -> Result<Vec<Arc<Account>>>;
    async fn range(&self, ctx: &Arc<dyn AccessContextT>, range: std::ops::Range<usize>) -> Result<Vec<Arc<Account>>>;
    async fn store(&self, ctx: &Arc<dyn AccessContextT>, data: &[&Account]) -> Result<()>;
    async fn remove(&self, ctx: &Arc<dyn AccessContextT>, id: &[AccountId]) -> Result<()>;
}

#[async_trait]
pub trait MetadataStore: Send + Sync {
    // async fn iter(self: Arc<Self>) -> Arc<dyn Iterator<Item = AccountId>>;
    async fn load(&self, ctx: &Arc<dyn AccessContextT>, id: &[AccountId]) -> Result<Vec<Metadata>>;
    async fn store(&self, ctx: &Arc<dyn AccessContextT>, data: &[&Metadata]) -> Result<()>;
    async fn remove(&self, ctx: &Arc<dyn AccessContextT>, id: &[AccountId]) -> Result<()>;
}

#[async_trait]
pub trait TransactionRecordStore: Send + Sync {
    async fn iter(self: Arc<Self>, options: IteratorOptions) -> Box<dyn Iterator<Item = TransactionRecordId>>;
    async fn load(&self, ctx: &Arc<dyn AccessContextT>, id: &[&TransactionRecordId]) -> Result<Vec<TransactionRecord>>;
    async fn store(&self, ctx: &Arc<dyn AccessContextT>, data: &[&TransactionRecord]) -> Result<()>;
    async fn remove(&self, ctx: &Arc<dyn AccessContextT>, id: &[&TransactionRecordId]) -> Result<()>;
}

#[async_trait]
// pub trait Interface: Sized + Send + Sync {
pub trait Interface: Send + Sync {
    async fn open(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()>;
    async fn close(&self, ctx: &Arc<dyn AccessContextT>) -> Result<()>;
    // ~~~
    async fn prv_key_data(self: Arc<Self>) -> Arc<dyn PrvKeyDataStore>;
    async fn account(self: Arc<Self>) -> Arc<dyn AccountStore>;
    async fn metadata(self: Arc<Self>) -> Arc<dyn MetadataStore>;
    async fn transaction_record(self: Arc<Self>) -> Arc<dyn TransactionRecordStore>;
}
