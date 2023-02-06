use crate::model::{UTXOChanges, UtxoSetByScriptPublicKey};

#[derive(Debug, Clone)]
///Notifications emitted by the UtxoIndex
pub enum UtxoIndexNotification {
    UtxoChanges(UtxoChangesNotification),
}

#[derive(Debug, Clone)]
///Notification which holds Added and Removed utxos of the utxoindex.  
pub struct UtxoChangesNotification {
    pub added: UtxoSetByScriptPublicKey,
    pub removed: UtxoSetByScriptPublicKey,
}

impl From<UTXOChanges> for UtxoChangesNotification {
    fn from(utxo_changes: UTXOChanges) -> Self {
        Self { added: utxo_changes.added, removed: utxo_changes.removed }
    }
}
