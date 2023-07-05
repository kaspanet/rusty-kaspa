use crate::imports::*;
use crate::utxo;

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
        url: String,
    },
    UtxoProcessor(utxo::Events),
}
