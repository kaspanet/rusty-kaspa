use crate::imports::*;
use kaspa_addresses::Prefix;
pub use kaspa_consensus_core::networktype::NetworkType;
use serde::{de, Deserializer, Serializer};
use std::{ops::Deref, str::FromStr};

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize, BorshSchema, PartialEq, Eq)]
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

impl From<kaspa_consensus_core::networktype::NetworkId> for NetworkId {
    fn from(net: kaspa_consensus_core::networktype::NetworkId) -> Self {
        NetworkId { network_type: net.network_type, suffix: net.suffix }
    }
}

impl FromStr for NetworkId {
    type Err = Error;
    fn from_str(network_name: &str) -> Result<Self, Self::Err> {
        let mut parts = network_name.split('-').fuse();
        let network_type = NetworkType::from_str(parts.next().unwrap_or_default())?;
        let suffix = parts.next().map(|x| u32::from_str(x).map_err(|_| Error::InvalidNetworkSuffix(x.to_string()))).transpose()?;
        // diallow network types without suffix (other than mainnet)
        // lack of suffix makes it impossible to distinguish between
        // multiple testnet networks
        if !matches!(network_type, NetworkType::Mainnet) && suffix.is_none() {
            return Err(Error::MissingNetworkSuffix(network_name.to_string()));
        }
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

impl Serialize for NetworkId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct NetworkIdVisitor;

impl<'de> de::Visitor<'de> for NetworkIdVisitor {
    type Value = NetworkId;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string containing network_type and optional suffix separated by a '-'")
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        NetworkId::from_str(value).map_err(|err| de::Error::custom(err.to_string()))
    }
}

impl<'de> Deserialize<'de> for NetworkId {
    fn deserialize<D>(deserializer: D) -> Result<NetworkId, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(NetworkIdVisitor)
    }
}

impl TryFrom<JsValue> for NetworkId {
    type Error = Error;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        let network_name = value.as_string().ok_or_else(|| Error::InvalidNetworkId(format!("{value:?}")))?;
        NetworkId::from_str(&network_name)
    }
}
