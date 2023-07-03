use crate::imports::*;
use crate::runtime::AccountId;
use crate::runtime::Balance;
use crate::storage::TransactionRecord;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "event", content = "data")]
pub enum Events {
    Connect(String),
    Disconnect(String),
    UtxoIndexNotEnabled,
    ServerStatus {
        #[serde(rename = "serverVersion")]
        server_version: String,
        #[serde(rename = "isSynced")]
        is_synced: bool,
        #[serde(rename = "hasUtxoIndex")]
        has_utxo_index: bool,
    },
    DAAScoreChange(u64),
    Credit {
        record: TransactionRecord,
    },
    Debit {
        record: TransactionRecord,
    },
    Balance {
        utxo_size: usize,
        balance: Option<Balance>,
        #[serde(rename = "accountId")]
        account_id: AccountId,
    },
}
