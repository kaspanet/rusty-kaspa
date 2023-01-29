use super::model::UtxoSetByScriptPublicKey;

#[derive(Debug, Clone)]
///Notifications emitted by the UtxoIndex
pub enum UtxoIndexNotification {
    UtxosChanged(UtxosChangedNotification),
}

#[derive(Debug, Clone)]
///Notification which holds Added and Removed utxos of the utxoindex.  
pub struct UtxosChangedNotification {
    pub added: UtxoSetByScriptPublicKey,
    pub removed: UtxoSetByScriptPublicKey,
}
