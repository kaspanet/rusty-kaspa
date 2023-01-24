use std::collections::HashMap;

use super::*;
use consensus_core::tx::ScriptPublicKey;

pub type UtxoSetByScriptPublicKey = HashMap<ScriptPublicKey, CompactUtxoCollection>;
