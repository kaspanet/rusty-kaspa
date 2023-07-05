use crate::imports::*;
use crate::result::Result;
// use crate::runtime::AccountId;
use crate::runtime::Balance;
use crate::storage::TransactionRecord;
use crate::utxo::processor::UtxoProcessorId;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "event", content = "data")]
pub enum Events {
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
        mature_utxo_size: usize,
        pending_utxo_size: usize,
        balance: Option<Balance>,
        // #[serde(rename = "accountId")]
        id: UtxoProcessorId,
    },
}

#[async_trait]
pub trait EventConsumer: Send + Sync {
    async fn notify(&self, event: Events) -> Result<()>;
}
