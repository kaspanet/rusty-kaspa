use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use kaspa_addresses::Prefix;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;
use wasm_bindgen::prelude::*;
use workflow_core::enums::u8_try_from;

#[derive(thiserror::Error, PartialEq, Eq, Debug, Clone)]
pub enum NetworkTypeError {
    #[error("Invalid network type: {0}")]
    InvalidNetworkType(String),
}

u8_try_from! {
    #[derive(Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema, PartialEq, Eq)]
    #[serde(rename_all = "lowercase")]
    #[wasm_bindgen]
    pub enum NetworkType {
        Mainnet,
        Testnet,
        Devnet,
        Simnet,
    }
}

impl NetworkType {
    pub fn default_rpc_port(&self) -> u16 {
        match self {
            NetworkType::Mainnet => 16110,
            NetworkType::Testnet => 16210,
            NetworkType::Simnet => 16510,
            NetworkType::Devnet => 16610,
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

#[derive(thiserror::Error, PartialEq, Eq, Debug, Clone)]
pub enum NetworkIdError {
    #[error("Invalid network name prefix: {0}. The expected prefix is 'kaspa'.")]
    InvalidPrefix(String),

    #[error(transparent)]
    InvalidNetworkType(#[from] NetworkTypeError),

    #[error("Invalid network suffix: {0}. Only 32 bits unsigned integer (u32) are supported.")]
    InvalidSuffix(String),

    #[error("Unexpected extra token: {0}.")]
    UnexpectedExtraToken(String),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkId {
    pub network_type: NetworkType,
    pub suffix: Option<u32>,
}

impl NetworkId {
    pub const fn new(network_type: NetworkType) -> Self {
        Self { network_type, suffix: None }
    }

    pub const fn with_suffix(network_type: NetworkType, suffix: u32) -> Self {
        Self { network_type, suffix: Some(suffix) }
    }

    pub fn name(&self) -> String {
        self.to_string()
    }

    pub fn default_p2p_port(&self) -> u16 {
        // We define the P2P port on the [`networkId`] type in order to adapt testnet ports according to testnet suffix,
        // hence avoiding repeatedly failing P2P handshakes between nodes on different networks. RPC does not have
        // this reasoning so we keep it on the same port in order to simplify RPC client management (hence [`default_rpc_port`]
        // is defined on the [`NetworkType`] struct
        match self.network_type {
            NetworkType::Mainnet => 16111,
            NetworkType::Testnet => match self.suffix {
                Some(10) => 16211,
                Some(11) => 16311,
                None | Some(_) => 16411,
            },
            NetworkType::Simnet => 16511,
            NetworkType::Devnet => 16611,
        }
    }

    pub fn iter() -> impl Iterator<Item = Self> {
        static NETWORK_IDS: [NetworkId; 5] = [
            NetworkId::new(NetworkType::Mainnet),
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            NetworkId::with_suffix(NetworkType::Testnet, 11),
            NetworkId::new(NetworkType::Devnet),
            NetworkId::new(NetworkType::Simnet),
        ];
        NETWORK_IDS.iter().copied()
    }
}

impl Deref for NetworkId {
    type Target = NetworkType;

    fn deref(&self) -> &Self::Target {
        &self.network_type
    }
}

impl From<NetworkType> for NetworkId {
    fn from(value: NetworkType) -> Self {
        Self::new(value)
    }
}

impl From<NetworkId> for Prefix {
    fn from(net: NetworkId) -> Self {
        (*net).into()
    }
}

impl FromStr for NetworkId {
    type Err = NetworkIdError;
    fn from_str(network_name: &str) -> Result<Self, Self::Err> {
        let mut parts = network_name.split('-').fuse();
        let prefix = parts.next().unwrap_or_default();
        if prefix != "kaspa" {
            return Err(NetworkIdError::InvalidPrefix(prefix.to_string()));
        }
        let network_type = NetworkType::from_str(parts.next().unwrap_or_default())?;
        let suffix = parts.next().map(|x| u32::from_str(x).map_err(|_| NetworkIdError::InvalidSuffix(x.to_string()))).transpose()?;
        match parts.next() {
            Some(extra_token) => Err(NetworkIdError::UnexpectedExtraToken(extra_token.to_string())),
            None => Ok(Self { network_type, suffix }),
        }
    }
}

impl Display for NetworkId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(suffix) = self.suffix {
            write!(f, "kaspa-{}-{}", self.network_type, suffix)
        } else {
            write!(f, "kaspa-{}", self.network_type)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_id_parse_roundtrip() {
        for nt in NetworkType::iter() {
            let ni = NetworkId::from(nt);
            let nis = NetworkId::with_suffix(nt, 1);
            assert_eq!(nt, *NetworkId::from_str(ni.to_string().as_str()).unwrap());
            assert_eq!(ni, NetworkId::from_str(ni.to_string().as_str()).unwrap());
            assert_eq!(nt, *NetworkId::from_str(nis.to_string().as_str()).unwrap());
            assert_eq!(nis, NetworkId::from_str(nis.to_string().as_str()).unwrap());

            assert_eq!(nis, NetworkId::from_str(nis.name().as_str()).unwrap());
        }
    }

    #[test]
    fn test_network_id_parse() {
        struct Test {
            name: &'static str,
            expr: &'static str,
            expected: Result<NetworkId, NetworkIdError>,
        }

        let tests = vec![
            Test { name: "Valid mainnet", expr: "kaspa-mainnet", expected: Ok(NetworkId::new(NetworkType::Mainnet)) },
            Test { name: "Valid testnet", expr: "kaspa-testnet-88", expected: Ok(NetworkId::with_suffix(NetworkType::Testnet, 88)) },
            Test { name: "Missing prefix", expr: "testnet", expected: Err(NetworkIdError::InvalidPrefix("testnet".to_string())) },
            Test { name: "Invalid prefix", expr: "K-testnet", expected: Err(NetworkIdError::InvalidPrefix("K".to_string())) },
            Test {
                name: "Missing network",
                expr: "kaspa-",
                expected: Err(NetworkTypeError::InvalidNetworkType("".to_string()).into()),
            },
            Test {
                name: "Invalid network",
                expr: "kaspa-gamenet",
                expected: Err(NetworkTypeError::InvalidNetworkType("gamenet".to_string()).into()),
            },
            Test { name: "Invalid suffix", expr: "kaspa-testnet-x", expected: Err(NetworkIdError::InvalidSuffix("x".to_string())) },
            Test {
                name: "Unexpected extra token",
                expr: "kaspa-testnet-10-x",
                expected: Err(NetworkIdError::UnexpectedExtraToken("x".to_string())),
            },
        ];

        for test in tests {
            assert_eq!(NetworkId::from_str(test.expr), test.expected, "{}: unexpected result", test.name);
        }
    }
}
