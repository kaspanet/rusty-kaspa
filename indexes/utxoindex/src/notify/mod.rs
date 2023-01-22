use super::model::{UTXOChanges, UtxoSetByScriptPublicKey};

#[derive(Debug, Clone)]
pub enum UtxoIndexNotification {
    UtxosChanged(UtxosChangedNotification),
    //TODO: circulating supply update notifications in rpc -> uncomment below when done.
    //CirculatingSupply(CirulatingSupply),
}

#[derive(Debug, Clone)]
pub struct UtxosChangedNotification {
    added: UtxoSetByScriptPublicKey,
    removed: UtxoSetByScriptPublicKey,
}

//TODO: circulating supply update notifications in rpc -> uncomment below when done.
/*
struct CirculatingSupplyNotification {
    circulating_supply: UtxoSetByScriptPublicKey,
}

impl CirculatingSupplyNotification {
    fn new(circulating_supply: CirulatingSupply) -> Self {
        Self { circulating_supply, }
    }
}
*/

impl From<UTXOChanges> for UtxosChangedNotification {
    fn from(utxo_changes: UTXOChanges) -> Self {
        Self { added: utxo_changes.added, removed: utxo_changes.removed }
    }
}
