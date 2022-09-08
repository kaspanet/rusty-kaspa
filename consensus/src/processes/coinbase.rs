use std::convert::TryInto;

use consensus_core::tx::Transaction;

const UINT64_LEN: usize = 8;
const UINT16_LEN: usize = 2;
const LENGTH_OF_SUBSIDY: usize = UINT64_LEN;
const LENGTH_OF_SCRIPT_PUB_KEY_LENGTH: usize = 1;
const LENGTH_OF_VERSION_SCRIPT_PUB_KEY: usize = UINT16_LEN;

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum CoinbaseError {
    #[error("coinbase payload length is {0} while the minimum allowed length is {1}")]
    PayloadLenBelowMin(usize, usize),

    #[error("coinbase payload length is {0} while the maximum allowed length is {1}")]
    PayloadLenAboveMax(usize, usize),

    #[error("coinbase payload script public key length is {0} while the maximum allowed length is {1}")]
    PayloadScriptPublicKeyLenAboveMax(u8, u8),

    #[error("coinbase payload script public key length is {0} while the maximum allowed length is {1}")]
    PayloadScriptPublicKeyLen(u8, u8),

    #[error("coinbase payload length is {0} bytes but it needs to be at least {1} bytes long in order of accomodating the script public key")]
    PayloadCantContainScriptPublicKey(usize, usize),
}

pub type CoinbaseResult<T> = std::result::Result<T, CoinbaseError>;

#[derive(Clone)]
pub struct CoinbaseManager {
    coinbase_payload_script_public_key_max_len: u8,
    max_coinbase_payload_len: usize,
}

impl CoinbaseManager {
    pub fn new(coinbase_payload_script_public_key_max_len: u8, max_coinbase_payload_len: usize) -> Self {
        Self { coinbase_payload_script_public_key_max_len, max_coinbase_payload_len }
    }

    pub fn validate_coinbase_payload_in_isolation_and_extract_blue_score(
        &self, coinbase: &Transaction,
    ) -> CoinbaseResult<u64> {
        let payload = &coinbase.payload;
        const MIN_LEN: usize =
            UINT64_LEN + LENGTH_OF_SUBSIDY + LENGTH_OF_VERSION_SCRIPT_PUB_KEY + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH;

        if payload.len() < MIN_LEN {
            return Err(CoinbaseError::PayloadLenBelowMin(coinbase.payload.len(), MIN_LEN));
        }

        if payload.len() > self.max_coinbase_payload_len {
            return Err(CoinbaseError::PayloadLenAboveMax(coinbase.payload.len(), self.max_coinbase_payload_len));
        }

        let blue_score = u64::from_le_bytes(payload[..UINT64_LEN].try_into().unwrap());
        let subsidy = u64::from_le_bytes(
            payload[UINT64_LEN..UINT64_LEN + LENGTH_OF_SUBSIDY]
                .try_into()
                .unwrap(),
        );

        // Because LENGTH_OF_VERSION_SCRIPT_PUB_KEY is two bytes and script_pub_key_version reads only one byte, there's one byte
        // in the middle where the miner can write any arbitrary data. This means the code cannot support script pub key version
        // higher than 255. This can be fixed only via a soft-fork.
        let script_pub_key_version = payload[UINT16_LEN + LENGTH_OF_SUBSIDY] as u16;
        let script_pub_key_len = payload[UINT16_LEN + LENGTH_OF_SUBSIDY + LENGTH_OF_VERSION_SCRIPT_PUB_KEY];
        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }

        if payload.len() < MIN_LEN + script_pub_key_len as usize {
            return Err(CoinbaseError::PayloadCantContainScriptPublicKey(payload.len(), script_pub_key_len as usize));
        }

        let script_pub_key_script = &payload[UINT64_LEN
            + LENGTH_OF_SUBSIDY
            + LENGTH_OF_VERSION_SCRIPT_PUB_KEY
            + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH
            ..UINT64_LEN
                + LENGTH_OF_SUBSIDY
                + LENGTH_OF_VERSION_SCRIPT_PUB_KEY
                + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH
                + script_pub_key_len as usize];

        Ok(blue_score)
    }
}
