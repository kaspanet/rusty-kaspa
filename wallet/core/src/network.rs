use crate::imports::*;
use kaspa_addresses::Prefix;
pub use kaspa_consensus_core::networktype::NetworkType;
use std::{ops::Deref, str::FromStr};

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

impl From<NetworkId> for NetworkType {
    fn from(net: NetworkId) -> Self {
        *net
    }
}

impl From<NetworkId> for kaspa_consensus_core::networktype::NetworkId {
    fn from(net: NetworkId) -> Self {
        kaspa_consensus_core::networktype::NetworkId { network_type: net.network_type, suffix: net.suffix }
    }
}

impl FromStr for NetworkId {
    type Err = Error;
    fn from_str(network_name: &str) -> Result<Self, Self::Err> {
        let mut parts = network_name.split('-').fuse();
        let network_type = NetworkType::from_str(parts.next().unwrap_or_default())?;
        let suffix = parts.next().map(|x| u32::from_str(x).map_err(|_| Error::InvalidNetworkSuffix(x.to_string()))).transpose()?;
        match parts.next() {
            Some(extra_token) => Err(Error::UnexpectedExtraSuffixToken(extra_token.to_string())),
            None => Ok(Self { network_type, suffix }),
        }
    }
}

impl std::fmt::Display for NetworkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(suffix) = self.suffix {
            write!(f, "{}-{}", self.network_type, suffix)
        } else {
            write!(f, "{}", self.network_type)
        }
    }
}
