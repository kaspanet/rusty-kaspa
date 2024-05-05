//!
//! Events emitted by the wallet framework. This includes various wallet,
//! account and transaction events as well as state and sync events
//! produced by the client RPC and the Kaspa node monitoring subsystems.
//!

use crate::imports::*;
use crate::storage::{Hint, PrvKeyDataInfo, StorageDescriptor, TransactionRecord, WalletDescriptor};
use crate::utxo::context::UtxoContextId;
use transaction::TransactionRecordNotification;

/// Sync state of the kaspad node
#[derive(Clone, Debug, Serialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type", content = "data")]
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
#[derive(Clone, Debug, Serialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type", content = "data")]
pub enum Events {
    WalletPing,
    /// Successful RPC connection
    Connect {
        #[serde(rename = "networkId")]
        network_id: NetworkId,
        /// Node RPC url on which connection
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
        /// Node RPC url on which connection
        /// has been established
        url: Option<String>,
    },
    /// [`SyncState`] notification posted
    /// when the node sync state changes
    SyncState {
        #[serde(rename = "syncState")]
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
        #[serde(rename = "prvKeyDataInfo")]
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
        /// Node RPC url on which connection
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
    DaaScoreChange {
        #[serde(rename = "currentDaaScore")]
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
        balance: Option<Balance>,
        /// If UtxoContext is bound to a Runtime Account, this
        /// field will contain the account id. Otherwise, it will
        /// contain a developer-assigned internal id.
        id: UtxoContextId,
    },
    /// Periodic metrics updates (on-request)
    Metrics {
        #[serde(rename = "networkId")]
        network_id: NetworkId,
        // #[serde(rename = "metricsData")]
        // metrics_data: MetricsData,
        metrics: MetricsUpdate,
    },
    /// A general wallet framework error, emitted when an unexpected
    /// error occurs within the wallet framework.
    Error {
        message: String,
    },
}

impl Events {
    pub fn kind(&self) -> String {
        EventKind::from(self).to_string()
    }

    pub fn to_js_value(&self) -> wasm_bindgen::JsValue {
        match self {
            Events::Pending { record }
            | Events::Reorg { record }
            | Events::Stasis { record }
            | Events::Maturity { record }
            | Events::Discovery { record } => TransactionRecordNotification::new(self.kind(), record.clone()).into(),
            _ => serde_wasm_bindgen::to_value(self).unwrap(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    All,
    Connect,
    Disconnect,
    UtxoIndexNotEnabled,
    SyncState,
    WalletStart,
    WalletHint,
    WalletOpen,
    WalletCreate,
    WalletReload,
    WalletError,
    WalletClose,
    PrvKeyDataCreate,
    AccountActivation,
    AccountDeactivation,
    AccountSelection,
    AccountCreate,
    AccountUpdate,
    ServerStatus,
    UtxoProcStart,
    UtxoProcStop,
    UtxoProcError,
    DaaScoreChange,
    Pending,
    Reorg,
    Stasis,
    Maturity,
    Discovery,
    Balance,
    Metrics,
    Error,
}

impl From<&Events> for EventKind {
    fn from(event: &Events) -> Self {
        match event {
            Events::WalletPing { .. } => EventKind::WalletStart,

            Events::Connect { .. } => EventKind::Connect,
            Events::Disconnect { .. } => EventKind::Disconnect,
            Events::UtxoIndexNotEnabled { .. } => EventKind::UtxoIndexNotEnabled,
            Events::SyncState { .. } => EventKind::SyncState,
            Events::WalletHint { .. } => EventKind::WalletHint,
            Events::WalletOpen { .. } => EventKind::WalletOpen,
            Events::WalletCreate { .. } => EventKind::WalletCreate,
            Events::WalletReload { .. } => EventKind::WalletReload,
            Events::WalletError { .. } => EventKind::WalletError,
            Events::WalletClose => EventKind::WalletClose,
            Events::PrvKeyDataCreate { .. } => EventKind::PrvKeyDataCreate,
            Events::AccountActivation { .. } => EventKind::AccountActivation,
            Events::AccountDeactivation { .. } => EventKind::AccountDeactivation,
            Events::AccountSelection { .. } => EventKind::AccountSelection,
            Events::AccountCreate { .. } => EventKind::AccountCreate,
            Events::AccountUpdate { .. } => EventKind::AccountUpdate,
            Events::ServerStatus { .. } => EventKind::ServerStatus,
            Events::UtxoProcStart => EventKind::UtxoProcStart,
            Events::UtxoProcStop => EventKind::UtxoProcStop,
            Events::UtxoProcError { .. } => EventKind::UtxoProcError,
            Events::DaaScoreChange { .. } => EventKind::DaaScoreChange,
            Events::Pending { .. } => EventKind::Pending,
            Events::Reorg { .. } => EventKind::Reorg,
            Events::Stasis { .. } => EventKind::Stasis,
            Events::Maturity { .. } => EventKind::Maturity,
            Events::Discovery { .. } => EventKind::Discovery,
            Events::Balance { .. } => EventKind::Balance,
            Events::Metrics { .. } => EventKind::Metrics,
            Events::Error { .. } => EventKind::Error,
        }
    }
}

impl FromStr for EventKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "*" => Ok(EventKind::All),
            "connect" => Ok(EventKind::Connect),
            "disconnect" => Ok(EventKind::Disconnect),
            "utxo-index-not-enabled" => Ok(EventKind::UtxoIndexNotEnabled),
            "sync-state" => Ok(EventKind::SyncState),
            "wallet-start" => Ok(EventKind::WalletStart),
            "wallet-hint" => Ok(EventKind::WalletHint),
            "wallet-open" => Ok(EventKind::WalletOpen),
            "wallet-create" => Ok(EventKind::WalletCreate),
            "wallet-reload" => Ok(EventKind::WalletReload),
            "wallet-error" => Ok(EventKind::WalletError),
            "wallet-close" => Ok(EventKind::WalletClose),
            "prv-key-data-create" => Ok(EventKind::PrvKeyDataCreate),
            "account-activation" => Ok(EventKind::AccountActivation),
            "account-deactivation" => Ok(EventKind::AccountDeactivation),
            "account-selection" => Ok(EventKind::AccountSelection),
            "account-create" => Ok(EventKind::AccountCreate),
            "account-update" => Ok(EventKind::AccountUpdate),
            "server-status" => Ok(EventKind::ServerStatus),
            "utxo-proc-start" => Ok(EventKind::UtxoProcStart),
            "utxo-proc-stop" => Ok(EventKind::UtxoProcStop),
            "utxo-proc-error" => Ok(EventKind::UtxoProcError),
            "daa-score-change" => Ok(EventKind::DaaScoreChange),
            "pending" => Ok(EventKind::Pending),
            "reorg" => Ok(EventKind::Reorg),
            "stasis" => Ok(EventKind::Stasis),
            "maturity" => Ok(EventKind::Maturity),
            "discovery" => Ok(EventKind::Discovery),
            "balance" => Ok(EventKind::Balance),
            "metrics" => Ok(EventKind::Metrics),
            "error" => Ok(EventKind::Error),
            _ => Err(Error::custom("Invalid event kind")),
        }
    }
}

impl TryFrom<JsValue> for EventKind {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self> {
        let s = js_value.as_string().ok_or_else(|| Error::custom("Invalid event kind"))?;
        EventKind::from_str(&s)
    }
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let str = match self {
            EventKind::All => "all",
            EventKind::WalletStart => "wallet-start",
            EventKind::Connect => "connect",
            EventKind::Disconnect => "disconnect",
            EventKind::UtxoIndexNotEnabled => "utxo-index-not-enabled",
            EventKind::SyncState => "sync-state",
            EventKind::WalletHint => "wallet-hint",
            EventKind::WalletOpen => "wallet-open",
            EventKind::WalletCreate => "wallet-create",
            EventKind::WalletReload => "wallet-reload",
            EventKind::WalletError => "wallet-error",
            EventKind::WalletClose => "wallet-close",
            EventKind::PrvKeyDataCreate => "prv-key-data-create",
            EventKind::AccountActivation => "account-activation",
            EventKind::AccountDeactivation => "account-deactivation",
            EventKind::AccountSelection => "account-selection",
            EventKind::AccountCreate => "account-create",
            EventKind::AccountUpdate => "account-update",
            EventKind::ServerStatus => "server-status",
            EventKind::UtxoProcStart => "utxo-proc-start",
            EventKind::UtxoProcStop => "utxo-proc-stop",
            EventKind::UtxoProcError => "utxo-proc-error",
            EventKind::DaaScoreChange => "daa-score-change",
            EventKind::Pending => "pending",
            EventKind::Reorg => "reorg",
            EventKind::Stasis => "stasis",
            EventKind::Maturity => "maturity",
            EventKind::Discovery => "discovery",
            EventKind::Balance => "balance",
            EventKind::Metrics => "metrics",
            EventKind::Error => "error",
        };

        write!(f, "{str}")
    }
}
