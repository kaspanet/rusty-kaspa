use crate::imports::*;
use crate::runtime::{AccountDescriptor, Balance};
use crate::storage::{Hint, PrvKeyDataInfo, StorageDescriptor, TransactionRecord, WalletDescriptor};
use crate::utxo::context::UtxoContextId;

/// Sync state of the kaspad node
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "sync", content = "state")]
pub enum SyncState {
    Proof {
        level: u64,
    },
    Headers {
        headers: u64,
        progress: u64,
    },
    Blocks {
        blocks: u64,
        progress: u64,
    },
    UtxoSync {
        chunks: u64,
        total: u64,
    },
    TrustSync {
        processed: u64,
        total: u64,
    },
    UtxoResync,
    /// General cases when the node is waiting
    /// for information from peers or waiting to
    /// connect to peers.
    NotSynced,
    /// Node is fully synced with the network.
    Synced,
}

impl SyncState {
    pub fn is_synced(&self) -> bool {
        matches!(self, SyncState::Synced)
    }
}

/// Events emitted by the wallet framework
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "event", content = "data")]
pub enum Events {
    /// Successful RPC connection
    Connect {
        #[serde(rename = "networkId")]
        network_id: NetworkId,
        /// Kaspa node RPC url on which connection
        /// has been established
        url: Option<String>,
    },
    /// RPC disconnection
    Disconnect {
        #[serde(rename = "networkId")]
        network_id: NetworkId,
        url: Option<String>,
    },
    /// A special event emitted if the connected node
    /// does not have UTXO index enabled
    UtxoIndexNotEnabled {
        /// Kaspa node RPC url on which connection
        /// has been established
        url: Option<String>,
    },
    /// [`SyncState`] notification posted
    /// when the node sync state changes
    SyncState {
        sync_state: SyncState,
    },
    /// Emitted after the wallet has loaded and
    /// contains anti-phishing 'hint' set by the user.
    WalletHint {
        hint: Option<Hint>,
    },
    /// Wallet has opened
    WalletOpen {
        wallet_descriptor: Option<WalletDescriptor>,
        account_descriptors: Option<Vec<AccountDescriptor>>,
    },
    WalletCreate {
        wallet_descriptor: WalletDescriptor,
        storage_descriptor: StorageDescriptor,
    },
    /// Wallet reload initiated (development only)
    WalletReload {
        wallet_descriptor: Option<WalletDescriptor>,
        account_descriptors: Option<Vec<AccountDescriptor>>,
    },
    /// Wallet open failure
    WalletError {
        message: String,
    },
    /// Wallet has been closed
    WalletClose,
    PrvKeyDataCreate {
        prv_key_data_info: PrvKeyDataInfo,
    },
    /// Accounts have been activated
    AccountActivation {
        ids: Vec<runtime::AccountId>,
    },
    /// Accounts have been deactivated
    AccountDeactivation {
        ids: Vec<runtime::AccountId>,
    },
    /// Account selection change (`None` if no account is selected)
    AccountSelection {
        id: Option<runtime::AccountId>,
    },
    /// Account has been created
    AccountCreate {
        account_descriptor: AccountDescriptor,
    },
    /// Account has been changed
    /// (emitted on new address generation)
    AccountUpdate {
        account_descriptor: AccountDescriptor,
    },
    /// Emitted after successful RPC connection
    /// after the initial state negotiation.
    ServerStatus {
        #[serde(rename = "networkId")]
        network_id: NetworkId,
        #[serde(rename = "serverVersion")]
        server_version: String,
        #[serde(rename = "isSynced")]
        is_synced: bool,
        /// Kaspa node RPC url on which connection
        /// has been established
        url: Option<String>,
    },

    /// Successful start of [`UtxoProcessor`](crate::utxo::processor::UtxoProcessor).
    /// This event signifies that the application can
    /// start interfacing with the UTXO processor.
    UtxoProcStart,
    /// [`UtxoProcessor`](crate::utxo::processor::UtxoProcessor) has shut down.
    UtxoProcStop,
    /// Occurs when UtxoProcessor has failed to connect to the node
    /// for an unknown reason. (general error trap)
    UtxoProcError {
        message: String,
    },
    /// DAA score change
    DAAScoreChange {
        current_daa_score: u64,
    },
    /// New incoming pending UTXO/transaction
    Pending {
        record: TransactionRecord,
    },
    /// Pending UTXO has been removed (reorg)
    Reorg {
        record: TransactionRecord,
    },
    /// Coinbase statis UTXO has been removed (reorg)
    /// NOTE: These transactions should be ignored by clients.
    Stasis {
        record: TransactionRecord,
    },
    /// Transaction has been confirmed
    Maturity {
        record: TransactionRecord,
    },
    /// Emitted when a transaction has been created and broadcasted
    /// by the Transaction [`Generator`](crate::tx::generator::Generator)
    Discovery {
        record: TransactionRecord,
    },
    /// UtxoContext (Account) balance update. Emitted for each
    /// balance change within the UtxoContext.
    Balance {
        // #[serde(rename = "matureUtxoSize")]
        // mature_utxo_size: usize,
        // #[serde(rename = "pendingUtxoSize")]
        // pending_utxo_size: usize,
        balance: Option<Balance>,
        /// If UtxoContext is bound to a Runtime Account, this
        /// field will contain the account id. Otherwise, it will
        /// contain a developer-assigned internal id.
        id: UtxoContextId,
    },
    Error {
        message: String,
    },
}
