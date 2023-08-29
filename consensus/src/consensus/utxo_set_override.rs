#[cfg(feature = "developer-mode")]
pub mod utxo_set_override {
    use std::sync::Arc;

    use itertools::Itertools;
    use kaspa_consensus_core::{
        api::ConsensusApi, config::Config, header::Header, muhash::MuHashExtensions, utxo::utxo_collection::UtxoCollection,
    };
    use kaspa_hashes::Hash;
    use kaspa_muhash::MuHash;

    use crate::consensus::Consensus;

    pub fn set_genesis_utxo_commitment_from_config(config: &mut Config) {
        let mut genesis_ms = MuHash::new();
        for (outpoint, entry) in config.initial_utxo_set.iter() {
            genesis_ms.add_utxo(outpoint, entry);
        }

        config.params.genesis.utxo_commitment = genesis_ms.finalize();
        let genesis_header: Header = (&config.params.genesis).into();
        config.params.genesis.hash = genesis_header.hash;
    }

    pub fn set_initial_utxo_set(initial_utxo_set: &UtxoCollection, consensus: Arc<Consensus>, genesis_hash: Hash) {
        let utxo_slice = &initial_utxo_set.iter().map(|(op, entry)| (*op, entry.clone())).collect_vec()[..];
        let mut ms = MuHash::new();
        consensus.append_imported_pruning_point_utxos(utxo_slice, &mut ms);
        consensus.import_pruning_point_utxo_set(genesis_hash, ms, true).unwrap();
    }
}

#[cfg(feature = "developer-mode")]
pub use utxo_set_override::*;
