use std::collections::HashMap;

use super::*;
use consensus_core::tx::ScriptPublicKey;

pub type UtxoSetByScriptPublicKey = HashMap<ScriptPublicKey, CompactUtxoCollection>;

pub struct UtxoSetDiffByScriptPublicKey {
    pub added: UtxoSetByScriptPublicKey,
    pub removed: UtxoSetByScriptPublicKey,
}

impl UtxoSetDiffByScriptPublicKey {
    pub fn new() -> Self {
        Self {
            added: UtxoSetByScriptPublicKey::new(),
            removed: UtxoSetByScriptPublicKey::new(),
        }
    }
}