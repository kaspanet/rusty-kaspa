use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use kaspa_addresses::Prefix;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;
use wasm_bindgen::prelude::*;

#[derive(thiserror::Error, PartialEq, Eq, Debug, Clone)]
pub enum NetworkTypeError {
    #[error("Invalid network type: {0}")]
    InvalidNetworkType(String),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[wasm_bindgen]
pub enum NetworkType {
    Mainnet,
    Testnet,
    Devnet,
    Simnet,
}

impl NetworkType {
    pub fn default_p2p_port(&self) -> u16 {
        match self {
            NetworkType::Mainnet => 16111,
            NetworkType::Testnet => 16211,
            NetworkType::Simnet => 16511,
            NetworkType::Devnet => 16611,
        }
    }

    pub fn default_rpc_port(&self) -> u16 {
        match self {
            NetworkType::Mainnet => 16110,
            NetworkType::Testnet => 16210,
            NetworkType::Simnet => 16510,
            NetworkType::Devnet => 16610,
        }
    }

    pub fn name(&self, suffix: Option<u32>) -> String {
        if let Some(suffix) = suffix {
            format!("kaspa-{}-{}", self, suffix)
        } else {
            format!("kaspa-{}", self)
        }
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        static NETWORK_TYPES: [NetworkType; 4] =
            [NetworkType::Mainnet, NetworkType::Testnet, NetworkType::Devnet, NetworkType::Simnet];
        NETWORK_TYPES.iter().copied()
    }
}

// #[wasm_bindgen]
// impl NetworkType {
//     pub fn as_address_prefix_str(&self) -> String {
//         let prefix : Prefix = self.clone().into();
//         prefix.to_string()
//     }
// }

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

impl From<NetworkType> for Prefix {
    fn from(network_type: NetworkType) -> Self {
        match network_type {
            NetworkType::Mainnet => Prefix::Mainnet,
            NetworkType::Testnet => Prefix::Testnet,
            NetworkType::Devnet => Prefix::Devnet,
            NetworkType::Simnet => Prefix::Simnet,
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
