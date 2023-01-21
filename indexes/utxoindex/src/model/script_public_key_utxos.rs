use std::collections::HashMap;

use super::*;
use consensus_core::tx::ScriptPublicKey;

pub type UtxoSetByScriptPublicKey = HashMap<ScriptPublicKey, CompactUtxoCollection>;

#[derive(Clone)]
pub struct UTXOChanges {
    pub added: UtxoSetByScriptPublicKey,
    pub removed: UtxoSetByScriptPublicKey,
}

impl UTXOChanges {
    pub fn new() -> Self {
        Self { added: UtxoSetByScriptPublicKey::new(), removed: UtxoSetByScriptPublicKey::new() }
    }
}
