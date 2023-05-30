pub mod store_names {
    pub const VIRTUAL_UTXO_SET: &[u8] = b"virtual-utxo-set";
    pub const REACHABILITY_RELATIONS_PREFIX: &[u8] = b"reachability-";
}

// Re-exports constants from consensus core for internal crate usage
pub use kaspa_consensus_core::config::constants::*;
pub(crate) use kaspa_consensus_core::constants::*;
