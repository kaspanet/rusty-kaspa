use consensus_core::BlockHashSet;

use super::super::model::utxo_index_changes::UtxoSetDiffByScriptPublicKey;

pub type UtxoIndexNotificationTypes = Vec<UtxoIndexNotificationType>;

pub enum UtxoIndexNotificationType {
    UtxoByScriptPublicKeyDiffNotificationType,
    CirculatingSupplyUpdateNotificationType,
    TipsUpdateNotificationType,
    All, //for ease of registering / unregistering
}
pub enum UtxoIndexNotification {
    UtxoByScriptPublicKeyDiffNotification(UtxoSetDiffByScriptPublicKey),
    CirculatingSupplyUpdateNotification(u64),
    TipsUpdateEvent(BlockHashSet),
}
