//!
//! Helpers for obtaining consensus parameters based
//! on the network type or address prefix.
//!

use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::{
    config::params::{Params, DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS},
    network::NetworkType,
};

/// find Consensus parameters for given Address
pub fn get_consensus_params_by_address(address: &Address) -> Params {
    match address.prefix {
        Prefix::Mainnet => MAINNET_PARAMS,
        Prefix::Testnet => TESTNET_PARAMS,
        Prefix::Simnet => SIMNET_PARAMS,
        _ => DEVNET_PARAMS,
    }
}

/// find Consensus parameters for given NetworkType
pub fn get_consensus_params_by_network(network: &NetworkType) -> Params {
    match network {
        NetworkType::Mainnet => MAINNET_PARAMS,
        NetworkType::Testnet => TESTNET_PARAMS,
        NetworkType::Simnet => SIMNET_PARAMS,
        _ => DEVNET_PARAMS,
    }
}
