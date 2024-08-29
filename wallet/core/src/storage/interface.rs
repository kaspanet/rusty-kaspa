//!
//! Wallet storage subsystem traits.
//!

use crate::imports::*;
use async_trait::async_trait;
use downcast::{downcast_sync, AnySync};

#[derive(Debug, Clone)]
pub struct WalletExportOptions {
    pub include_transactions: bool,
}

#[wasm_bindgen(typescript_custom_section)]
const TS_WALLET_DESCRIPTOR: &'static str = r#"
/**
 * Wallet storage information.
 * 
 * @category Wallet API
 */
export interface IWalletDescriptor {
    title?: string;
    filename: string;
}
"#;

/// @category Wallet API
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[wasm_bindgen(inspectable)]
pub struct WalletDescriptor {
    #[wasm_bindgen(getter_with_clone)]
    pub title: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub filename: String,
}

impl WalletDescriptor {
    pub fn new(title: Option<String>, filename: String) -> Self {
        Self { title, filename }
    }
}

#[wasm_bindgen(typescript_custom_section)]
const TS_STORAGE_DESCRIPTOR: &'static str = r#"
/**
 * Wallet storage information.
 */
export interface IStorageDescriptor {
    kind: string;
    data: string;
}
"#;

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "kind", content = "data")]
pub enum StorageDescriptor {
    Resident,
    Internal(String),
    Other(String),
}

impl std::fmt::Display for StorageDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageDescriptor::Resident => write!(f, "memory(resident)"),
            StorageDescriptor::Internal(path) => write!(f, "{path}"),
            StorageDescriptor::Other(other) => write!(f, "{other}"),
        }
    }
}

pub type StorageStream<T> = Pin<Box<dyn Stream<Item = Result<T>> + Send>>;

#[async_trait]
pub trait PrvKeyDataStore: Send + Sync {
    async fn is_empty(&self) -> Result<bool>;
    async fn iter(&self) -> Result<StorageStream<Arc<PrvKeyDataInfo>>>;
    async fn load_key_info(&self, id: &PrvKeyDataId) -> Result<Option<Arc<PrvKeyDataInfo>>>;
    async fn load_key_data(&self, wallet_secret: &Secret, id: &PrvKeyDataId) -> Result<Option<PrvKeyData>>;
    async fn store(&self, wallet_secret: &Secret, data: PrvKeyData) -> Result<()>;
    async fn remove(&self, wallet_secret: &Secret, id: &PrvKeyDataId) -> Result<()>;
}

#[async_trait]
pub trait AccountStore: Send + Sync {
    async fn is_empty(&self) -> Result<bool>;
    async fn iter(
        &self,
        prv_key_data_id_filter: Option<PrvKeyDataId>,
    ) -> Result<StorageStream<(Arc<AccountStorage>, Option<Arc<AccountMetadata>>)>>;
    async fn len(&self, prv_key_data_id_filter: Option<PrvKeyDataId>) -> Result<usize>;
    async fn load_single(&self, ids: &AccountId) -> Result<Option<(Arc<AccountStorage>, Option<Arc<AccountMetadata>>)>>;
    async fn load_multiple(&self, ids: &[AccountId]) -> Result<Vec<(Arc<AccountStorage>, Option<Arc<AccountMetadata>>)>>;
    async fn store_single(&self, account: &AccountStorage, metadata: Option<&AccountMetadata>) -> Result<()>;
    async fn store_multiple(&self, data: Vec<(AccountStorage, Option<AccountMetadata>)>) -> Result<()>;
    async fn remove(&self, id: &[&AccountId]) -> Result<()>;
    async fn update_metadata(&self, metadata: Vec<AccountMetadata>) -> Result<()>;
}

#[async_trait]
pub trait AddressBookStore: Send + Sync {
    async fn is_empty(&self) -> Result<bool> {
        Err(Error::NotImplemented)
    }
    async fn iter(&self) -> Result<StorageStream<Arc<AddressBookEntry>>> {
        Err(Error::NotImplemented)
    }
    async fn search(&self, _search: &str) -> Result<Vec<Arc<AddressBookEntry>>> {
        Err(Error::NotImplemented)
    }
}

pub struct TransactionRangeResult {
    pub transactions: Vec<Arc<TransactionRecord>>,
    pub total: u64,
}

#[async_trait]
pub trait TransactionRecordStore: Send + Sync {
    async fn transaction_id_iter(&self, binding: &Binding, network_id: &NetworkId) -> Result<StorageStream<Arc<TransactionId>>>;
    async fn transaction_data_iter(&self, binding: &Binding, network_id: &NetworkId) -> Result<StorageStream<Arc<TransactionRecord>>>;
    async fn load_range(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        filter: Option<Vec<TransactionKind>>,
        range: std::ops::Range<usize>,
    ) -> Result<TransactionRangeResult>;

    async fn load_single(&self, binding: &Binding, network_id: &NetworkId, id: &TransactionId) -> Result<Arc<TransactionRecord>>;
    async fn load_multiple(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        ids: &[TransactionId],
    ) -> Result<Vec<Arc<TransactionRecord>>>;

    async fn store(&self, transaction_records: &[&TransactionRecord]) -> Result<()>;
    async fn remove(&self, binding: &Binding, network_id: &NetworkId, ids: &[&TransactionId]) -> Result<()>;

    async fn store_transaction_note(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        id: TransactionId,
        note: Option<String>,
    ) -> Result<()>;
    async fn store_transaction_metadata(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        id: TransactionId,
        metadata: Option<String>,
    ) -> Result<()>;
}

#[derive(Debug)]
pub struct CreateArgs {
    pub title: Option<String>,
    pub filename: Option<String>,
    pub encryption_kind: EncryptionKind,
    pub user_hint: Option<Hint>,
    pub overwrite_wallet: bool,
}

impl CreateArgs {
    pub fn new(
        title: Option<String>,
        filename: Option<String>,
        encryption_kind: EncryptionKind,
        user_hint: Option<Hint>,
        overwrite_wallet: bool,
    ) -> Self {
        Self { title, filename, encryption_kind, user_hint, overwrite_wallet }
    }
}

#[derive(Debug)]
pub struct OpenArgs {
    pub filename: Option<String>,
}

impl OpenArgs {
    pub fn new(filename: Option<String>) -> Self {
        Self { filename }
    }
}

#[async_trait]
pub trait Interface: Send + Sync + AnySync {
    /// enumerate all wallets available in the storage
    async fn wallet_list(&self) -> Result<Vec<WalletDescriptor>>;

    /// check if a wallet is currently open
    fn is_open(&self) -> bool;

    /// return storage information string (file location)
    fn location(&self) -> Result<StorageDescriptor>;

    /// returns the name of the currently open wallet or none
    fn descriptor(&self) -> Option<WalletDescriptor>;

    /// encryption used by the currently open wallet
    fn encryption_kind(&self) -> Result<EncryptionKind>;

    /// rename the currently open wallet (title or the filename)
    async fn rename(&self, wallet_secret: &Secret, title: Option<&str>, filename: Option<&str>) -> Result<()>;

    /// change the secret of the currently open wallet
    async fn change_secret(&self, old_wallet_secret: &Secret, new_wallet_secret: &Secret) -> Result<()>;

    /// checks if the wallet storage is present
    async fn exists(&self, name: Option<&str>) -> Result<bool>;

    /// initialize wallet storage
    async fn create(&self, wallet_secret: &Secret, args: CreateArgs) -> Result<WalletDescriptor>;

    /// establish an open state (load wallet data cache, connect to the database etc.)
    async fn open(&self, wallet_secret: &Secret, args: OpenArgs) -> Result<()>;

    /// suspend commit operations until flush() is called
    async fn batch(&self) -> Result<()>;

    /// flush resumes commit operations previously suspended by `suspend()`
    async fn flush(&self, wallet_secret: &Secret) -> Result<()>;

    /// commit any changes changes to storage
    async fn commit(&self, wallet_secret: &Secret) -> Result<()>;

    /// stop the storage subsystem
    async fn close(&self) -> Result<()>;

    /// export the wallet data
    async fn wallet_export(&self, wallet_secret: &Secret, options: WalletExportOptions) -> Result<Vec<u8>>;

    /// import the wallet data
    async fn wallet_import(&self, wallet_secret: &Secret, serialized_wallet_storage: &[u8]) -> Result<WalletDescriptor>;

    // ~~~

    // phishing hint (user-created text string identifying authenticity of the wallet)
    async fn get_user_hint(&self) -> Result<Option<Hint>>;
    async fn set_user_hint(&self, hint: Option<Hint>) -> Result<()>;

    // ~~~
    fn as_prv_key_data_store(&self) -> Result<Arc<dyn PrvKeyDataStore>>;
    fn as_account_store(&self) -> Result<Arc<dyn AccountStore>>;
    fn as_address_book_store(&self) -> Result<Arc<dyn AddressBookStore>>;
    fn as_transaction_record_store(&self) -> Result<Arc<dyn TransactionRecordStore>>;
}

downcast_sync!(dyn Interface);
