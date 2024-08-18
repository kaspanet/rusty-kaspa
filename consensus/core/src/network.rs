use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Prefix;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;
use wasm_bindgen::convert::TryFromJsValue;
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

#[derive(thiserror::Error, PartialEq, Eq, Debug, Clone)]
pub enum NetworkTypeError {
    #[error("Invalid network type: {0}")]
    InvalidNetworkType(String),
}

/// @category Consensus
#[derive(Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
#[wasm_bindgen]
pub enum NetworkType {
    Mainnet,
    Testnet,
    Devnet,
    Simnet,
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

    pub fn default_borsh_rpc_port(&self) -> u16 {
        match self {
            NetworkType::Mainnet => 17110,
            NetworkType::Testnet => 17210,
            NetworkType::Simnet => 17510,
            NetworkType::Devnet => 17610,
        }
    }

    pub fn default_json_rpc_port(&self) -> u16 {
        match self {
            NetworkType::Mainnet => 18110,
            NetworkType::Testnet => 18210,
            NetworkType::Simnet => 18510,
            NetworkType::Devnet => 18610,
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
        match network_type.to_lowercase().as_str() {
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

impl TryFrom<&NetworkTypeT> for NetworkType {
    type Error = NetworkTypeError;
    fn try_from(value: &NetworkTypeT) -> Result<Self, Self::Error> {
        if let Ok(network_id) = NetworkId::try_cast_from(value) {
            Ok(network_id.network_type())
        } else if let Some(network_type) = value.as_string() {
            Self::from_str(&network_type)
        } else if let Ok(network_type) = NetworkType::try_from_js_value(JsValue::from(value)) {
            Ok(network_type)
        } else {
            Err(NetworkTypeError::InvalidNetworkType(format!("{value:?}")))
        }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = "Network", typescript_type = "NetworkType | NetworkId | string")]
    #[derive(Debug)]
    pub type NetworkTypeT;
}

impl TryFrom<&NetworkTypeT> for Prefix {
    type Error = NetworkIdError;
    fn try_from(value: &NetworkTypeT) -> Result<Self, Self::Error> {
        Ok(NetworkType::try_from(value)?.into())
    }
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum NetworkIdError {
    #[error("Invalid network name prefix: {0}. The expected prefix is 'kaspa'.")]
    InvalidPrefix(String),

    #[error(transparent)]
    InvalidNetworkType(#[from] NetworkTypeError),

    #[error("Invalid network suffix: {0}. Only 32 bits unsigned integer (u32) are supported.")]
    InvalidSuffix(String),

    #[error("Unexpected extra token: {0}.")]
    UnexpectedExtraToken(String),

    #[error("Missing network suffix: '{0}'")]
    MissingNetworkSuffix(String),

    #[error("Network suffix required for network type: '{0}'")]
    NetworkSuffixRequired(String),

    #[error("Invalid network id: '{0}'")]
    InvalidNetworkId(String),

    #[error(transparent)]
    Wasm(#[from] workflow_wasm::error::Error),
}

impl From<NetworkIdError> for JsValue {
    fn from(err: NetworkIdError) -> Self {
        JsValue::from_str(&err.to_string())
    }
}

///
/// NetworkId is a unique identifier for a kaspa network instance.
/// It is composed of a network type and an optional suffix.
///
/// @category Consensus
///
#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq, Hash, Ord, PartialOrd, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct NetworkId {
    #[wasm_bindgen(js_name = "type")]
    pub network_type: NetworkType,
    #[wasm_bindgen(js_name = "suffix")]
    pub suffix: Option<u32>,
}

impl NetworkId {
    pub const fn new(network_type: NetworkType) -> Self {
        if !matches!(network_type, NetworkType::Mainnet | NetworkType::Devnet | NetworkType::Simnet) {
            panic!("network suffix required for this network type");
        }

        Self { network_type, suffix: None }
    }

    pub fn try_new(network_type: NetworkType) -> Result<Self, NetworkIdError> {
        if !matches!(network_type, NetworkType::Mainnet | NetworkType::Devnet | NetworkType::Simnet) {
            return Err(NetworkIdError::NetworkSuffixRequired(network_type.to_string()));
        }

        Ok(Self { network_type, suffix: None })
    }

    pub const fn with_suffix(network_type: NetworkType, suffix: u32) -> Self {
        Self { network_type, suffix: Some(suffix) }
    }

    pub fn network_type(&self) -> NetworkType {
        self.network_type
    }

    pub fn is_mainnet(&self) -> bool {
        self.network_type == NetworkType::Mainnet
    }

    pub fn suffix(&self) -> Option<u32> {
        self.suffix
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

    /// Returns a textual description of the network prefixed with `kaspa-`
    pub fn to_prefixed(&self) -> String {
        format!("kaspa-{}", self)
    }

    pub fn from_prefixed(prefixed: &str) -> Result<Self, NetworkIdError> {
        if let Some(stripped) = prefixed.strip_prefix("kaspa-") {
            Self::from_str(stripped)
        } else {
            Err(NetworkIdError::InvalidPrefix(prefixed.to_string()))
        }
    }
}

impl Deref for NetworkId {
    type Target = NetworkType;

    fn deref(&self) -> &Self::Target {
        &self.network_type
    }
}

impl TryFrom<NetworkType> for NetworkId {
    type Error = NetworkIdError;
    fn try_from(value: NetworkType) -> Result<Self, Self::Error> {
        Self::try_new(value)
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

impl FromStr for NetworkId {
    type Err = NetworkIdError;
    fn from_str(network_name: &str) -> Result<Self, Self::Err> {
        let mut parts = network_name.split('-').fuse();
        let network_type = NetworkType::from_str(parts.next().unwrap_or_default())?;
        let suffix = parts.next().map(|x| u32::from_str(x).map_err(|_| NetworkIdError::InvalidSuffix(x.to_string()))).transpose()?;
        // Disallow testnet network without suffix.
        // Lack of suffix makes it impossible to distinguish between
        // multiple testnet networks
        if !matches!(network_type, NetworkType::Mainnet | NetworkType::Devnet | NetworkType::Simnet) && suffix.is_none() {
            return Err(NetworkIdError::MissingNetworkSuffix(network_name.to_string()));
        }
        match parts.next() {
            Some(extra_token) => Err(NetworkIdError::UnexpectedExtraToken(extra_token.to_string())),
            None => Ok(Self { network_type, suffix }),
        }
    }
}

impl Display for NetworkId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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

#[wasm_bindgen]
impl NetworkId {
    #[wasm_bindgen(constructor)]
    pub fn ctor(value: &JsValue) -> Result<NetworkId, NetworkIdError> {
        Ok(NetworkId::try_cast_from(value)?.into_owned())
    }

    #[wasm_bindgen(getter, js_name = "id")]
    pub fn js_id(&self) -> String {
        self.to_string()
    }

    #[wasm_bindgen(js_name = "toString")]
    pub fn js_to_string(&self) -> String {
        self.to_string()
    }

    #[wasm_bindgen(js_name = "addressPrefix")]
    pub fn js_address_prefix(&self) -> String {
        Prefix::from(self.network_type).to_string()
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "NetworkId | string")]
    pub type NetworkIdT;
}

impl TryFrom<&JsValue> for NetworkId {
    type Error = NetworkIdError;
    fn try_from(value: &JsValue) -> Result<Self, Self::Error> {
        Self::try_owned_from(value)
    }
}

impl TryFrom<JsValue> for NetworkId {
    type Error = NetworkIdError;
    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        Self::try_owned_from(value)
    }
}

impl TryCastFromJs for NetworkId {
    type Error = NetworkIdError;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if let Some(network_id) = value.as_ref().as_string() {
                Ok(NetworkId::from_str(&network_id)?)
            } else {
                Err(NetworkIdError::InvalidNetworkId(format!("{:?}", value.as_ref())))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_id_parse_roundtrip() {
        for nt in NetworkType::iter() {
            if matches!(nt, NetworkType::Mainnet | NetworkType::Devnet | NetworkType::Simnet) {
                let ni = NetworkId::try_from(nt).expect("failed to create network id");
                assert_eq!(nt, *NetworkId::from_str(ni.to_string().as_str()).unwrap());
                assert_eq!(ni, NetworkId::from_str(ni.to_string().as_str()).unwrap());
            }
            let nis = NetworkId::with_suffix(nt, 1);
            assert_eq!(nt, *NetworkId::from_str(nis.to_string().as_str()).unwrap());
            assert_eq!(nis, NetworkId::from_str(nis.to_string().as_str()).unwrap());

            assert_eq!(nis, NetworkId::from_str(nis.to_string().as_str()).unwrap());
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
            Test { name: "Valid mainnet", expr: "mainnet", expected: Ok(NetworkId::new(NetworkType::Mainnet)) },
            Test { name: "Valid testnet", expr: "testnet-88", expected: Ok(NetworkId::with_suffix(NetworkType::Testnet, 88)) },
            Test { name: "Missing network", expr: "", expected: Err(NetworkTypeError::InvalidNetworkType("".to_string()).into()) },
            Test {
                name: "Invalid network",
                expr: "gamenet",
                expected: Err(NetworkTypeError::InvalidNetworkType("gamenet".to_string()).into()),
            },
            Test { name: "Invalid suffix", expr: "testnet-x", expected: Err(NetworkIdError::InvalidSuffix("x".to_string())) },
            Test {
                name: "Unexpected extra token",
                expr: "testnet-10-x",
                expected: Err(NetworkIdError::UnexpectedExtraToken("x".to_string())),
            },
        ];

        for test in tests {
            let Test { name, expr, expected } = test;
            match NetworkId::from_str(expr) {
                Ok(nid) => assert_eq!(nid, expected.unwrap(), "{}: unexpected result", name),
                Err(err) => assert_eq!(err.to_string(), expected.unwrap_err().to_string(), "{}: unexpected error", name),
            }
        }
    }
}
