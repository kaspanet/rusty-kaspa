use serde::{de, Deserializer, Serializer};

use crate::imports::*;
use std::{fmt, str::FromStr};
// use convert_case::{Case, Casing};

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PathParams {
    pub encoding: WrpcEncoding,
    pub network: NetworkId,
}

impl PathParams {
    pub fn new(encoding: WrpcEncoding, network: NetworkId) -> Self {
        Self { encoding, network }
    }

    pub fn iter() -> impl Iterator<Item = PathParams> {
        NetworkId::iter().flat_map(move |network_id| WrpcEncoding::iter().map(move |encoding| PathParams::new(*encoding, network_id)))
    }
}

impl fmt::Display for PathParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.encoding.to_string().to_lowercase(), self.network)
    }
}

// ---

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    // Accessible via a query string like "?access=utxo-index+tx-index+block-dag+metrics+visualizer+mining"
    pub access: Option<AccessList>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum AccessType {
    Transact,   // UTXO and TX index, submit transaction, single mempool entry
    Mempool,    // Full mempool data access
    BlockDag,   // Access to Blocks
    Network,    // Network data access (peers, ban, etc.)
    Metrics,    // Access to Metrics
    Visualizer, // Access to Visualization data feeds
    Mining,     // Access to submit block, GBT, etc.
}

impl AccessType {
    pub fn iter() -> impl Iterator<Item = AccessType> {
        [
            AccessType::Transact,
            AccessType::Mempool,
            AccessType::BlockDag,
            AccessType::Network,
            AccessType::Metrics,
            AccessType::Visualizer,
            AccessType::Mining,
        ]
        .into_iter()
    }
}

impl fmt::Display for AccessType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AccessType::Transact => "transact",
            AccessType::Mempool => "mempool",
            AccessType::BlockDag => "block-dag",
            AccessType::Network => "network",
            AccessType::Metrics => "metrics",
            AccessType::Visualizer => "visualizer",
            AccessType::Mining => "mining",
        };
        write!(f, "{s}")
    }
}

impl FromStr for AccessType {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "transact" => Ok(AccessType::Transact),
            "mempool" => Ok(AccessType::Mempool),
            "block-dag" => Ok(AccessType::BlockDag),
            "network" => Ok(AccessType::Network),
            "metrics" => Ok(AccessType::Metrics),
            "visualizer" => Ok(AccessType::Visualizer),
            "mining" => Ok(AccessType::Mining),
            _ => Err(format!("Invalid access type: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AccessList {
    pub access: Vec<AccessType>,
}

impl std::fmt::Display for AccessList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.access.iter().map(|access| access.to_string()).collect::<Vec<_>>().join(" "))
    }
}

impl FromStr for AccessList {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let access = s.split(' ').map(|s| s.parse::<AccessType>()).collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(AccessList { access })
    }
}

impl Serialize for AccessList {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct AccessListVisitor;
impl<'de> de::Visitor<'de> for AccessListVisitor {
    type Value = AccessList;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string containing list of permissions separated by a '+'")
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        AccessList::from_str(value).map_err(|err| de::Error::custom(err.to_string()))
    }
}

impl<'de> Deserialize<'de> for AccessList {
    fn deserialize<D>(deserializer: D) -> std::result::Result<AccessList, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(AccessListVisitor)
    }
}
