use std::collections::HashMap;

use super::*;
use consensus_core::tx::ScriptPublicKey;

pub type UtxoSetByScriptPublicKey = HashMap<ScriptPublicKey, CompactUtxoCollection>;

pub struct UtxoSetDiffByScriptPublicKey {
    pub added: UtxoByScriptPublicKey,
    pub removed: UtxoByScriptPublicKey,
}

impl UtxoSetDiffByScriptPublicKey {
    pub fn new() -> Self {
        Self {
            added: UtxoByScriptPublicKey::new(),
            removed: UtxoByScriptPublicKey::new(),
        }
    }
}