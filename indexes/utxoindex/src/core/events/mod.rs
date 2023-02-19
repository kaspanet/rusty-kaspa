use crate::model::UtxoSetByScriptPublicKey;
use std::sync::Arc;

pub enum UtxoIndexEvent {
    UtxosChanged(Arc<UtxosChangedEvent>),
}

#[derive(Debug, Clone)]
///Notification which holds Added and Removed utxos of the utxoindex.  
pub struct UtxosChangedEvent {
    pub added: Arc<UtxoSetByScriptPublicKey>,
    pub removed: Arc<UtxoSetByScriptPublicKey>,
}
