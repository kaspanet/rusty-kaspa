//!
//! Wallet framework network parameters that control maturity
//! durations and other transaction related properties.
//!

use crate::imports::*;
use kaspa_consensus_core::mass::Kip9Version;

#[derive(Debug)]
pub struct NetworkParams {
    pub coinbase_transaction_maturity_period_daa: AtomicU64,
    pub coinbase_transaction_stasis_period_daa: u64,
    pub user_transaction_maturity_period_daa: AtomicU64,
    pub kip9_version: Kip9Version,
    pub additional_compound_transaction_mass: u64,
}

impl NetworkParams {
    #[inline]
    pub fn coinbase_transaction_maturity_period_daa(&self) -> u64 {
        self.coinbase_transaction_maturity_period_daa.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn coinbase_transaction_stasis_period_daa(&self) -> u64 {
        self.coinbase_transaction_stasis_period_daa
    }

    #[inline]
    pub fn user_transaction_maturity_period_daa(&self) -> u64 {
        self.user_transaction_maturity_period_daa.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn kip9_version(&self) -> Kip9Version {
        self.kip9_version
    }

    #[inline]
    pub fn additional_compound_transaction_mass(&self) -> u64 {
        self.additional_compound_transaction_mass
    }

    pub fn set_coinbase_transaction_maturity_period_daa(&self, value: u64) {
        self.coinbase_transaction_maturity_period_daa.store(value, Ordering::Relaxed);
    }

    pub fn set_user_transaction_maturity_period_daa(&self, value: u64) {
        self.user_transaction_maturity_period_daa.store(value, Ordering::Relaxed);
    }
}

static MAINNET_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(100),
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: AtomicU64::new(10),
    kip9_version: Kip9Version::Beta,
    additional_compound_transaction_mass: 100,
});

static TESTNET10_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(100),
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: AtomicU64::new(10),
    kip9_version: Kip9Version::Beta,
    additional_compound_transaction_mass: 100,
});

static TESTNET11_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(1_000),
    coinbase_transaction_stasis_period_daa: 500,
    user_transaction_maturity_period_daa: AtomicU64::new(100),
    kip9_version: Kip9Version::Alpha,
    additional_compound_transaction_mass: 100,
});

static SIMNET_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(100),
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: AtomicU64::new(10),
    kip9_version: Kip9Version::Alpha,
    additional_compound_transaction_mass: 0,
});

static DEVNET_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(100),
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: AtomicU64::new(10),
    kip9_version: Kip9Version::Beta,
    additional_compound_transaction_mass: 0,
});

impl NetworkParams {
    pub fn from(value: NetworkId) -> &'static NetworkParams {
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
