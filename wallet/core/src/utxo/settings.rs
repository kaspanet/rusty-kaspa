//!
//! Wallet framework network parameters that control maturity
//! durations and other transaction related properties.
//!

use crate::imports::*;

#[derive(Debug)]
pub struct Inner {
    pub coinbase_transaction_maturity_period_daa: AtomicU64,
    pub coinbase_transaction_stasis_period_daa: u64,
    pub user_transaction_maturity_period_daa: AtomicU64,
    pub mass_combination_strategy: MassCombinationStrategy,
    pub additional_compound_transaction_mass: u64,
}

#[derive(Debug, Clone)]
pub struct NetworkParams {
    inner: Arc<Inner>,
}

impl NetworkParams {
    #[inline]
    pub fn coinbase_transaction_maturity_period_daa(&self) -> u64 {
        self.inner.coinbase_transaction_maturity_period_daa.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn coinbase_transaction_stasis_period_daa(&self) -> u64 {
        self.inner.coinbase_transaction_stasis_period_daa
    }

    #[inline]
    pub fn user_transaction_maturity_period_daa(&self) -> u64 {
        self.inner.user_transaction_maturity_period_daa.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn mass_combination_strategy(&self) -> MassCombinationStrategy {
        self.inner.mass_combination_strategy
    }

    #[inline]
    pub fn additional_compound_transaction_mass(&self) -> u64 {
        self.inner.additional_compound_transaction_mass
    }

    pub fn set_coinbase_transaction_maturity_period_daa(&self, value: u64) {
        self.inner.coinbase_transaction_maturity_period_daa.store(value, Ordering::Relaxed);
    }

    pub fn set_user_transaction_maturity_period_daa(&self, value: u64) {
        self.inner.user_transaction_maturity_period_daa.store(value, Ordering::Relaxed);
    }
}

lazy_static::lazy_static! {
    pub static ref MAINNET_NETWORK_PARAMS: NetworkParams = NetworkParams {
        inner: Arc::new(Inner {
            coinbase_transaction_maturity_period_daa: AtomicU64::new(100),
            coinbase_transaction_stasis_period_daa: 50,
            user_transaction_maturity_period_daa: AtomicU64::new(10),
            mass_combination_strategy: MassCombinationStrategy::Max,
            additional_compound_transaction_mass: 0,
        }),
    };
}

lazy_static::lazy_static! {
    pub static ref TESTNET10_NETWORK_PARAMS: NetworkParams = NetworkParams {
        inner: Arc::new(Inner {
            coinbase_transaction_maturity_period_daa: AtomicU64::new(100),
            coinbase_transaction_stasis_period_daa: 50,
            user_transaction_maturity_period_daa: AtomicU64::new(10),
            mass_combination_strategy: MassCombinationStrategy::Max,
            additional_compound_transaction_mass: 0,
        }),
    };
}

lazy_static::lazy_static! {
    pub static ref TESTNET11_NETWORK_PARAMS: NetworkParams = NetworkParams {
        inner: Arc::new(Inner {
            coinbase_transaction_maturity_period_daa: AtomicU64::new(1_000),
            coinbase_transaction_stasis_period_daa: 500,
            user_transaction_maturity_period_daa: AtomicU64::new(100),
            mass_combination_strategy: MassCombinationStrategy::Max,
            additional_compound_transaction_mass: 100,
        }),
    };
}

lazy_static::lazy_static! {
    pub static ref SIMNET_NETWORK_PARAMS: NetworkParams = NetworkParams {
        inner: Arc::new(Inner {
            coinbase_transaction_maturity_period_daa: AtomicU64::new(100),
            coinbase_transaction_stasis_period_daa: 50,
            user_transaction_maturity_period_daa: AtomicU64::new(10),
            mass_combination_strategy: MassCombinationStrategy::Max,
            additional_compound_transaction_mass: 0,
        }),
    };
}

lazy_static::lazy_static! {
    pub static ref DEVNET_NETWORK_PARAMS: NetworkParams = NetworkParams {
        inner: Arc::new(Inner {
            coinbase_transaction_maturity_period_daa: AtomicU64::new(100),
            coinbase_transaction_stasis_period_daa: 50,
            user_transaction_maturity_period_daa: AtomicU64::new(10),
            mass_combination_strategy: MassCombinationStrategy::Max,
            additional_compound_transaction_mass: 0,
        }),
    };
}

impl From<NetworkId> for NetworkParams {
    fn from(value: NetworkId) -> Self {
        match value.network_type {
            NetworkType::Mainnet => MAINNET_NETWORK_PARAMS.clone(),
            NetworkType::Testnet => match value.suffix {
                Some(10) => TESTNET10_NETWORK_PARAMS.clone(),
                Some(11) => TESTNET11_NETWORK_PARAMS.clone(),
                Some(x) => panic!("Testnet suffix {} is not supported", x),
                None => panic!("Testnet suffix not provided"),
            },
            NetworkType::Devnet => DEVNET_NETWORK_PARAMS.clone(),
            NetworkType::Simnet => SIMNET_NETWORK_PARAMS.clone(),
        }
    }
}

/// Set the coinbase transaction maturity period DAA score for a given network.
/// This controls the DAA period after which the user transactions are considered mature
/// and the wallet subsystem emits the transaction maturity event.
pub fn set_coinbase_transaction_maturity_period_daa(network_id: &NetworkId, value: u64) {
    let network_params = NetworkParams::from(*network_id);
    if value <= network_params.coinbase_transaction_stasis_period_daa() {
        panic!(
            "Coinbase transaction maturity period must be greater than the stasis period of {} DAA",
            network_params.coinbase_transaction_stasis_period_daa()
        );
    }
    network_params.set_coinbase_transaction_maturity_period_daa(value);
}

/// Set the user transaction maturity period DAA score for a given network.
/// This controls the DAA period after which the user transactions are considered mature
/// and the wallet subsystem emits the transaction maturity event.
pub fn set_user_transaction_maturity_period_daa(network_id: &NetworkId, value: u64) {
    let network_params = NetworkParams::from(*network_id);
    if value == 0 {
        panic!("User transaction maturity period must be greater than 0");
    }
    network_params.set_user_transaction_maturity_period_daa(value);
}
