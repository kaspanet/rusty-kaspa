use crate::imports::*;
use crate::runtime::Account;
use crate::utxo::UtxoProcessorCore;

pub struct UtxoProcessorContext {
    pub core: UtxoProcessorCore,
    pub account: Arc<Account>,
}
