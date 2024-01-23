//!
//! Events emitted by the wallet framework. This includes various wallet,
//! account and transaction events as well as state and sync events
//! produced by the client RPC and the Kaspa node monitoring subsystems.
//!

use crate::imports::*;
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
        ids: Vec<AccountId>,
    },
    /// Accounts have been deactivated
    AccountDeactivation {
        ids: Vec<AccountId>,
    },
    /// Account selection change (`None` if no account is selected)
    AccountSelection {
        id: Option<AccountId>,
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

    /// Successful start of [`UtxoProcessor`].
    /// This event signifies that the application can
    /// start interfacing with the UTXO processor.
    UtxoProcStart,
    /// [`UtxoProcessor`] has shut down.
    UtxoProcStop,
    /// Occurs when UtxoProcessor has failed to connect to the node
    /// for an unknown reason. Can also occur during general unexpected
    /// UtxoProcessor processing errors, such as node disconnection
    /// then submitting an outgoing transaction. This is a general
    /// error trap for logging purposes and is safe to ignore.
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
    /// Coinbase stasis UTXO has been removed (reorg)
    /// NOTE: These transactions should be ignored by clients.
    Stasis {
        record: TransactionRecord,
    },
    /// Transaction has been confirmed
    Maturity {
        record: TransactionRecord,
    },
    /// Emitted when a transaction has been discovered
    /// during the UTXO scan. This event is generated
    /// when a runtime [`Account`]
    /// initiates address monitoring and performs
    /// an initial scan of the UTXO set.
    ///
    /// This event is emitted when UTXOs are
    /// registered with the UtxoContext using the
    /// [`UtxoContext::extend_from_scan()`](UtxoContext::extend_from_scan) method.
    ///
    /// NOTE: if using runtime [`Wallet`],
    /// the wallet will not emit this event if it detects
    /// that the transaction already exist in its transaction
    /// record set. If it doesn't, the wallet will create
    /// such record and emit this event only once. These
    /// transactions will be subsequently available when
    /// accessing the wallet's transaction record set.
    /// (i.e. when using runtime Wallet, this event can be
    /// ignored and transaction record can be accessed from
    /// the transaction history instead).
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
    /// A general wallet framework error, emitted when an unexpected
    /// error occurs within the wallet framework.
    Error {
        message: String,
    },
}
