use super::model::UtxoSetDiffByScriptPublicKey;
use consensus_core::BlockHashSet;
pub enum UtxoIndexEvent {
    UtxoByScriptPublicKeyDiffEvent(UtxoSetDiffByScriptPublicKey),
    CirculatingSupplyDiffEvent(i64),
    TipsUpdateEvent(BlockHashSet),
}
