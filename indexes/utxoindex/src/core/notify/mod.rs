use super::UtxoSetByScriptPublicKey;

#[derive(Debug, Clone)]
pub enum UtxoIndexNotification {
    UtxosChanged(UtxosChangedNotification),
    //TODO: circulating supply update notifications in rpc -> uncomment below when done.
    //CirculatingSupply(CirulatingSupply),
}

#[derive(Debug, Clone)]
pub struct UtxosChangedNotification {
    pub added: UtxoSetByScriptPublicKey,
    pub removed: UtxoSetByScriptPublicKey,
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
