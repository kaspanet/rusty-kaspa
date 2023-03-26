use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use kaspa_addresses::Prefix;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;

#[derive(thiserror::Error, PartialEq, Eq, Debug, Clone)]
pub enum NetworkTypeError {
    #[error("Invalid network type: {0}")]
    InvalidNetworkType(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "lowercase")]
pub enum NetworkType {
    Mainnet,
    Testnet,
    Devnet,
    Simnet,
}

impl NetworkType {
    pub fn port(&self) -> u16 {
        match self {
            NetworkType::Mainnet => 16110,
            NetworkType::Testnet => 16210,
            NetworkType::Simnet => 16510,
            NetworkType::Devnet => 16610,
        }
    }
}

impl TryFrom<Prefix> for NetworkType {
    type Error = NetworkTypeError;
    fn try_from(prefix: Prefix) -> Result<Self, Self::Error> {
        match prefix {
            Prefix::Mainnet => Ok(NetworkType::Mainnet),
            Prefix::Testnet => Ok(NetworkType::Testnet),
            Prefix::Simnet => Ok(NetworkType::Simnet),
            Prefix::Devnet => Ok(NetworkType::Devnet),
            #[allow(unreachable_patterns)]
            #[cfg(test)]
            _ => Err(NetworkTypeError::InvalidNetworkType(prefix.to_string())),
        }
    }
}

impl FromStr for NetworkType {
    type Err = NetworkTypeError;
    fn from_str(network_type: &str) -> Result<Self, Self::Err> {
        match network_type {
            "mainnet" => Ok(NetworkType::Mainnet),
            "testnet" => Ok(NetworkType::Testnet),
            "simnet" => Ok(NetworkType::Simnet),
            "devnet" => Ok(NetworkType::Devnet),
            _ => Err(NetworkTypeError::InvalidNetworkType(network_type.to_string())),
        }
    }
}

impl Display for NetworkType {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            NetworkType::Mainnet => "mainnet",
            NetworkType::Testnet => "testnet",
            NetworkType::Simnet => "simnet",
            NetworkType::Devnet => "devnet",
        };
        f.write_str(s)
    }
}
