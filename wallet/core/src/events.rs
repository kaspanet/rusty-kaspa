use crate::imports::*;
use crate::runtime::Balance;
use crate::storage::Hint;
use crate::storage::TransactionRecord;
use crate::utxo::context::UtxoContextId;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "sync", content = "state")]
pub enum SyncState {
    Proof { level: u64 },
    Headers { headers: u64, progress: u64 },
    Blocks { blocks: u64, progress: u64 },
    UtxoSync { chunks: u64, total: u64 },
    TrustSync { processed: u64, total: u64 },
    UtxoResync,
    NotSynced,
    Synced,
}

impl SyncState {
    pub fn is_synced(&self) -> bool {
        matches!(self, SyncState::Synced)
    }
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
    SyncState(SyncState),
    WalletHint {
        hint: Option<Hint>,
    },
    WalletOpen,
    WalletReload,
    WalletClose,
    ServerStatus {
        #[serde(rename = "networkId")]
        network_id: NetworkId,
        #[serde(rename = "serverVersion")]
        server_version: String,
        #[serde(rename = "isSynced")]
        is_synced: bool,
        url: String,
    },

    /// Successful start of [`UtxoProcessor`](super::utxo::processor::UtxoProcessor)
    UtxoProcStart,
    UtxoProcStop,
    UtxoProcError(String),
    /// DAA score change
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
