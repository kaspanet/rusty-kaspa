//!
//! Wallet framework network parameters that control maturity
//! durations and other transaction related properties.
//!

use crate::imports::*;

#[derive(Debug)]
pub struct NetworkParams {
    pub coinbase_transaction_maturity_period_daa: u64,
    pub coinbase_transaction_stasis_period_daa: u64,
    pub user_transaction_maturity_period_daa: u64,
    pub mass_combination_strategy: MassCombinationStrategy,
    pub additional_compound_transaction_mass: u64,
}

pub const MAINNET_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 100,
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: 10,
    mass_combination_strategy: MassCombinationStrategy::Add,
    additional_compound_transaction_mass: 0,
};

pub const TESTNET10_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 100,
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: 10,
    mass_combination_strategy: MassCombinationStrategy::Add,
    additional_compound_transaction_mass: 100,
};

pub const TESTNET11_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 1_000,
    coinbase_transaction_stasis_period_daa: 500,
    user_transaction_maturity_period_daa: 100,
    mass_combination_strategy: MassCombinationStrategy::Add,
    additional_compound_transaction_mass: 100,
};

pub const DEVNET_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 100,
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: 10,
    mass_combination_strategy: MassCombinationStrategy::Add,
    additional_compound_transaction_mass: 0,
};

pub const SIMNET_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 100,
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: 10,
    mass_combination_strategy: MassCombinationStrategy::Add,
    additional_compound_transaction_mass: 0,
};

impl From<NetworkId> for &'static NetworkParams {
    fn from(value: NetworkId) -> Self {
        match value.network_type {
            NetworkType::Mainnet => &MAINNET_NETWORK_PARAMS,
            NetworkType::Testnet => match value.suffix {
                Some(10) => &TESTNET10_NETWORK_PARAMS,
                Some(11) => &TESTNET11_NETWORK_PARAMS,
                Some(x) => panic!("Testnet suffix {} is not supported", x),
                None => panic!("Testnet suffix not provided"),
            },
            NetworkType::Devnet => &DEVNET_NETWORK_PARAMS,
            NetworkType::Simnet => &SIMNET_NETWORK_PARAMS,
        }
    }
}

impl From<NetworkId> for NetworkParams {
    fn from(value: NetworkId) -> Self {
        match value.network_type {
            NetworkType::Mainnet => MAINNET_NETWORK_PARAMS,
            NetworkType::Testnet => match value.suffix {
                Some(10) => TESTNET10_NETWORK_PARAMS,
                Some(11) => TESTNET11_NETWORK_PARAMS,
                Some(x) => panic!("Testnet suffix {} is not supported", x),
                None => panic!("Testnet suffix not provided"),
            },
            NetworkType::Devnet => DEVNET_NETWORK_PARAMS,
            NetworkType::Simnet => SIMNET_NETWORK_PARAMS,
        }
    }
}
