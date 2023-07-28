use crate::imports::*;
use crate::runtime::Balance;
use crate::storage::Hint;
use crate::storage::TransactionRecord;
use crate::utxo::context::UtxoContextId;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
// #[serde(tag = "state", content = "progress")]
pub enum SyncState {
    Proof { level: u64 },
    Headers { headers: u64, progress: u64 },
    Blocks { blocks: u64, progress: u64 },
    UtxoSync { chunks: u64, total: u64 },
    TrustSync { processed: u64, total: u64 },
    UtxoResync,
    Synced,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "event", content = "data")]
pub enum Events {
    Connect {
        #[serde(rename = "networkId")]
        network_id: NetworkId,
        url: String,
    },
    Disconnect {
        #[serde(rename = "networkId")]
        network_id: NetworkId,
        url: String,
    },
    UtxoIndexNotEnabled,
    NodeSync {
        #[serde(rename = "isSynced")]
        is_synced: bool,
    },
    SyncState {
        state: SyncState,
    },
    WalletHint {
        hint: Option<Hint>,
    },
    WalletLoaded,
    ServerStatus {
        #[serde(rename = "networkId")]
        network_id: NetworkId,
        #[serde(rename = "serverVersion")]
        server_version: String,
        #[serde(rename = "isSynced")]
        is_synced: bool,
        url: String,
    },
    UtxoProcStart,
    UtxoProcStop,
    UtxoProcError(String),

    // UtxoProcessor(utxo::Events),
    DAAScoreChange(u64),
    // New pending transaction
    Pending {
        record: TransactionRecord,
    },
    // Removal of a pending UTXO
    Reorg {
        record: TransactionRecord,
    },
    // The outbound transaction not known to us
    External {
        record: TransactionRecord,
    },
    Maturity {
        record: TransactionRecord,
    },
    Debit {
        record: TransactionRecord,
    },
    Balance {
        #[serde(rename = "matureUtxoSize")]
        mature_utxo_size: usize,
        #[serde(rename = "pendingUtxoSize")]
        pending_utxo_size: usize,
        balance: Option<Balance>,
        id: UtxoContextId,
    },
}

// #[async_trait]
// pub trait EventConsumer: Send + Sync {
//     async fn notify(&self, event: Events) -> Result<()>;
// }
