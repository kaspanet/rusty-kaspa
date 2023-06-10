use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use kaspa_addresses::Prefix;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;

#[derive(thiserror::Error, PartialEq, Eq, Debug, Clone)]
pub enum NetworkTypeError {
    #[error("Invalid network type: {0}")]
    InvalidNetworkType(String),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
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

    pub fn iter() -> impl Iterator<Item = Self> {
        static NETWORK_TYPES: [NetworkType; 4] =
            [NetworkType::Mainnet, NetworkType::Testnet, NetworkType::Devnet, NetworkType::Simnet];
        NETWORK_TYPES.iter().copied()
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
pub enum NetworkInstanceError {
    #[error("Invalid network name prefix: {0}. The expected prefix is 'kaspa'.")]
    InvalidPrefix(String),

    #[error(transparent)]
    InvalidNetworkType(#[from] NetworkTypeError),

    #[error("Invalid network suffix: {0}. Only 32 bits unsigned integer (u32) are supported.")]
    InvalidSuffix(String),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub struct NetworkInstance {
    pub network_type: NetworkType,
    pub suffix: Option<u32>,
}

impl NetworkInstance {
    pub fn new(network_type: NetworkType, suffix: Option<u32>) -> Self {
        Self { network_type, suffix }
    }

    pub fn name(&self) -> String {
        if let Some(suffix) = self.suffix {
            format!("kaspa-{}-{}", self.network_type, suffix)
        } else {
            format!("kaspa-{}", self.network_type)
        }
    }
}

impl Deref for NetworkInstance {
    type Target = NetworkType;

    fn deref(&self) -> &Self::Target {
        &self.network_type
    }
}

impl From<NetworkType> for NetworkInstance {
    fn from(value: NetworkType) -> Self {
        Self::new(value, None)
    }
}

impl From<NetworkInstance> for Prefix {
    fn from(net: NetworkInstance) -> Self {
        (*net).into()
    }
}

impl FromStr for NetworkInstance {
    type Err = NetworkInstanceError;
    fn from_str(network_name: &str) -> Result<Self, Self::Err> {
        let mut parts = network_name.split('-');
        let prefix = parts.next().unwrap_or_default();
        if prefix != "kaspa" {
            return Err(NetworkInstanceError::InvalidPrefix(prefix.to_string()));
        }
        let network_type = NetworkType::from_str(parts.next().unwrap_or_default())?;
        let suffix =
            parts.next().map(|x| u32::from_str(x).map_err(|_| NetworkInstanceError::InvalidSuffix(x.to_string()))).transpose()?;
        Ok(Self::new(network_type, suffix))
    }
}

impl Display for NetworkInstance {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_instance_parse_roundtrip() {
        for nt in NetworkType::iter() {
            let ni = NetworkInstance::from(nt);
            let nis = NetworkInstance::new(nt, Some(1));
            assert_eq!(nt, *NetworkInstance::from_str(ni.to_string().as_str()).unwrap());
            assert_eq!(ni, NetworkInstance::from_str(ni.to_string().as_str()).unwrap());
            assert_eq!(nt, *NetworkInstance::from_str(nis.to_string().as_str()).unwrap());
            assert_eq!(nis, NetworkInstance::from_str(nis.to_string().as_str()).unwrap());
        }
    }

    #[test]
    fn test_network_instance_parse() {
        struct Test {
            name: &'static str,
            expr: &'static str,
            expected: Result<NetworkInstance, NetworkInstanceError>,
        }

        let tests = vec![
            Test { name: "Valid mainnet", expr: "kaspa-mainnet", expected: Ok(NetworkInstance::new(NetworkType::Mainnet, None)) },
            Test {
                name: "Valid testnet",
                expr: "kaspa-testnet-88",
                expected: Ok(NetworkInstance::new(NetworkType::Testnet, Some(88))),
            },
            Test {
                name: "Missing prefix",
                expr: "testnet",
                expected: Err(NetworkInstanceError::InvalidPrefix("testnet".to_string())),
            },
            Test { name: "Invalid prefix", expr: "K-testnet", expected: Err(NetworkInstanceError::InvalidPrefix("K".to_string())) },
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
            Test {
                name: "Invalid suffix",
                expr: "kaspa-testnet-x",
                expected: Err(NetworkInstanceError::InvalidSuffix("x".to_string())),
            },
        ];

        for test in tests {
            assert_eq!(NetworkInstance::from_str(test.expr), test.expected, "{}: unexpected result", test.name);
        }
    }
}
