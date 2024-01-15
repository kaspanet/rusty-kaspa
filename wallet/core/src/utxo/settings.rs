//!
//! Wallet framework network parameters that control maturity
//! durations and other transaction related properties.
//!

use crate::imports::*;

pub struct NetworkParams {
    pub coinbase_transaction_maturity_period_daa: u64,
    pub coinbase_transaction_stasis_period_daa: u64,
    pub user_transaction_maturity_period_daa: u64,
}

pub const MAINNET_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 100,
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: 10,
};

pub const TESTNET10_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 100,
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: 10,
};

pub const TESTNET11_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 1_000,
    coinbase_transaction_stasis_period_daa: 500,
    user_transaction_maturity_period_daa: 100,
};

pub const DEVNET_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 100,
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: 10,
};

pub const SIMNET_NETWORK_PARAMS: NetworkParams = NetworkParams {
    coinbase_transaction_maturity_period_daa: 100,
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: 10,
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
