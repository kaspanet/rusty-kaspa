use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use thiserror::Error;
use wasm_bindgen::convert::FromWasmAbi;
use wasm_bindgen::prelude::*;

mod bech32;

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum AddressError {
    #[error("Invalid prefix {0}")]
    InvalidPrefix(String),

    #[error("Prefix is missing")]
    MissingPrefix,

    #[error("Invalid version {0}")]
    InvalidVersion(u8),

    #[error("Invalid character {0}")]
    DecodingError(char),

    #[error("Checksum is invalid")]
    BadChecksum,
}

#[derive(
    PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema,
)]
// #[wasm_bindgen]
pub enum Prefix {
    #[serde(rename = "kaspa")]
    Mainnet,
    #[serde(rename = "kaspatest")]
    Testnet,
    #[serde(rename = "kaspasim")]
    Simnet,
    #[serde(rename = "kaspadev")]
    Devnet,
    #[cfg(test)]
    A,
    #[cfg(test)]
    B,
}

impl Prefix {
    fn as_str(&self) -> &'static str {
        match self {
            Prefix::Mainnet => "kaspa",
            Prefix::Testnet => "kaspatest",
            Prefix::Simnet => "kaspasim",
            Prefix::Devnet => "kaspadev",
            #[cfg(test)]
            Prefix::A => "a",
            #[cfg(test)]
            Prefix::B => "b",
        }
    }

    #[inline(always)]
    fn is_test(&self) -> bool {
        #[cfg(not(test))]
        return false;
        #[cfg(test)]
        matches!(self, Prefix::A | Prefix::B)
    }
}

impl Display for Prefix {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<&str> for Prefix {
    type Error = AddressError;

    fn try_from(prefix: &str) -> Result<Self, Self::Error> {
        match prefix {
            "kaspa" => Ok(Prefix::Mainnet),
            "kaspatest" => Ok(Prefix::Testnet),
            "kaspasim" => Ok(Prefix::Simnet),
            "kaspadev" => Ok(Prefix::Devnet),
            #[cfg(test)]
            "a" => Ok(Prefix::A),
            #[cfg(test)]
            "b" => Ok(Prefix::B),
            _ => Err(AddressError::InvalidPrefix(prefix.to_string())),
        }
    }
}

///
///  Kaspa `Address` version (`PubKey`, `PubKey ECDSA`, `ScriptHash`)
///
#[derive(
    PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema,
)]
#[repr(u8)]
#[wasm_bindgen(js_name = "AddressVersion")]
pub enum Version {
    /// PubKey addresses always have the version byte set to 0
    PubKey = 0,
    /// PubKey ECDSA addresses always have the version byte set to 1
    PubKeyECDSA = 1,
    /// ScriptHash addresses always have the version byte set to 8
    ScriptHash = 8,
}

impl Version {
    pub fn public_key_len(&self) -> usize {
        match self {
            Version::PubKey => 32,
            Version::PubKeyECDSA => 33,
            Version::ScriptHash => 32,
        }
    }
}

impl TryFrom<u8> for Version {
    type Error = AddressError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Version::PubKey),
            1 => Ok(Version::PubKeyECDSA),
            8 => Ok(Version::ScriptHash),
            _ => Err(AddressError::InvalidVersion(value)),
        }
    }
}

impl ToString for Version {
    fn to_string(&self) -> String {
        match self {
            Version::PubKey => "PubKey",
            Version::PubKeyECDSA => "PubKeyECDSA",
            Version::ScriptHash => "ScriptHash",
        }
        .to_string()
    }
}

/// Size of the payload vector of an address.
///
/// This size is the smallest SmallVec supported backing store size greater or equal to the largest
/// possible payload, which is 33 for [`Version::PubKeyECDSA`].
pub const PAYLOAD_VECTOR_SIZE: usize = 36;

/// Used as the underlying type for address payload, optimized for the largest version length (33).
pub type PayloadVec = SmallVec<[u8; PAYLOAD_VECTOR_SIZE]>;

/// Kaspa `Address` struct that serializes to and from an address format string: `kaspa:qz0s...t8cv`.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, Hash)]
#[wasm_bindgen(inspectable)]
pub struct Address {
    #[wasm_bindgen(skip)]
    pub prefix: Prefix,
    #[wasm_bindgen(skip)]
    pub version: Version,
    #[wasm_bindgen(skip)]
    pub payload: PayloadVec,
}

impl Address {
    pub fn new(prefix: Prefix, version: Version, payload: &[u8]) -> Self {
        if !prefix.is_test() {
            assert_eq!(payload.len(), version.public_key_len());
        }
        Self { prefix, payload: PayloadVec::from_slice(payload), version }
    }
}

#[wasm_bindgen]
impl Address {
    #[wasm_bindgen(constructor)]
    pub fn constructor(address: &str) -> Address {
        address.try_into().unwrap_or_else(|err| panic!("Address::constructor() - address error `{}`: {err}", address))
    }

    /// Convert an address to a string.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_str(&self) -> String {
        self.into()
    }

    // /// Convert an address to a string.
    // #[wasm_bindgen(js_name = toJSON1)]
    // pub fn to_json(&self) -> String {
    //     self.to_string()
    // }

    #[wasm_bindgen(getter)]
    pub fn version(&self) -> String {
        self.version.to_string()
    }

    #[wasm_bindgen(getter)]
    pub fn prefix(&self) -> String {
        self.prefix.to_string()
    }

    // #[wasm_bindgen(getter, js_name = "networkType")]
    // pub fn network_type(&self) -> NetworkType {
    //     self.prefix.into()
    // }

    // #[wasm_bindgen(setter, js_name = "networkType")]
    // pub fn set_network_type(&mut self, network_type : NetworkType) {
    //     self.prefix = network_type.into();
    // }

    #[wasm_bindgen(setter)]
    pub fn set_prefix(&mut self, prefix: &str) {
        self.prefix = Prefix::try_from(prefix).unwrap_or_else(|err| panic!("Address::prefix() - invalid prefix `{prefix}`: {err}"));
    }

    #[wasm_bindgen(getter)]
    pub fn payload(&self) -> String {
        self.encode_payload()
    }
}

impl Display for Address {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

//
// Borsh serializers need to be manually implemented for `Address` since
// smallvec does not currently support Borsh
//

impl BorshSerialize for Address {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.prefix, writer)?;
        borsh::BorshSerialize::serialize(&self.version, writer)?;
        // Vectors and slices are all serialized internally the same way
        borsh::BorshSerialize::serialize(&self.payload.as_slice(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for Address {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        // Deserialize into vec first since we have no custom smallvec support
        let prefix: Prefix = borsh::BorshDeserialize::deserialize(buf)?;
        let version: Version = borsh::BorshDeserialize::deserialize(buf)?;
        let payload: Vec<u8> = borsh::BorshDeserialize::deserialize(buf)?;
        Ok(Self::new(prefix, version, &payload))
    }
}

impl BorshSchema for Address {
    fn add_definitions_recursively(
        definitions: &mut std::collections::HashMap<borsh::schema::Declaration, borsh::schema::Definition>,
    ) {
        let fields = borsh::schema::Fields::NamedFields(std::vec![
            ("prefix".to_string(), <Prefix>::declaration()),
            ("version".to_string(), <Version>::declaration()),
            ("payload".to_string(), <Vec<u8>>::declaration())
        ]);
        let definition = borsh::schema::Definition::Struct { fields };
        Self::add_definition(Self::declaration(), definition, definitions);
        <Prefix>::add_definitions_recursively(definitions);
        <Version>::add_definitions_recursively(definitions);
        // `<Vec<u8>>` can be safely used as scheme definition for smallvec. See comments above.
        <Vec<u8>>::add_definitions_recursively(definitions);
    }

    fn declaration() -> borsh::schema::Declaration {
        "Address".to_string()
    }
}

impl From<Address> for String {
    fn from(address: Address) -> Self {
        (&address).into()
    }
}

impl From<&Address> for String {
    fn from(address: &Address) -> Self {
        format!("{}:{}", address.prefix, address.encode_payload())
    }
}

impl TryFrom<String> for Address {
    type Error = AddressError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.split_once(':') {
            Some((prefix, payload)) => Self::decode_payload(prefix.try_into()?, payload),
            None => Err(AddressError::MissingPrefix),
        }
    }
}

impl TryFrom<&str> for Address {
    type Error = AddressError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.split_once(':') {
            Some((prefix, payload)) => Self::decode_payload(prefix.try_into()?, payload),
            None => Err(AddressError::MissingPrefix),
        }
    }
}

impl Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // let s = <std::string::String as Deserialize>::deserialize(deserializer)?;
        // Ok(s.try_into().map_err(serde::de::Error::custom)?)
        let address = deserializer.deserialize_any(AddressVisitor)?;
        Ok(address)
    }
}

struct AddressVisitor;

impl<'de> serde::de::Visitor<'de> for AddressVisitor {
    type Value = Address;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "valid address as string or Address object.")
    }

    fn visit_str<E>(self, str: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Address::try_from(str).map_err(|_| serde::de::Error::invalid_value(serde::de::Unexpected::Str(str), &self))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let key = map.next_key::<String>()?.ok_or(serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self))?;

        if key.eq("ptr") {
            let value = map.next_value::<u32>()?;
            return Ok(unsafe { Address::from_abi(value) });
        }

        if key.eq("version") || key.eq("prefix") || key.eq("payload") {
            let mut set = HashMap::new();
            let value = map.next_value::<String>().map_err(|_| serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self))?;
            set.insert(key, value);

            let key = map.next_key::<String>()?.ok_or(serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self))?;
            let value = map.next_value::<String>().map_err(|_| serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self))?;
            set.insert(key, value);

            let key = map.next_key::<String>()?.ok_or(serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self))?;
            let value = map.next_value::<String>().map_err(|_| serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self))?;
            set.insert(key, value);

            if !set.contains_key("version") || !set.contains_key("prefix") || !set.contains_key("payload") {
                return Err(serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self));
            }

            let prefix = set.get("prefix").unwrap();
            let payload = set.get("payload").unwrap();

            let address_str = format!("{prefix}:{payload}");

            return Address::try_from(address_str).map_err(|e| serde::de::Error::custom(e.to_string()));
        }

        Err(serde::de::Error::invalid_value(serde::de::Unexpected::Map, &self))
        //Err(serde::de::Error::invalid_value(serde::de::Unexpected::Str(&format!("Invalid address: {{{key:?}:{value:?}}}")), &self))
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    fn cases() -> Vec<(Address, &'static str)> {
        // cspell:disable
        vec![
            (Address::new(Prefix::A, Version::PubKey, b""), "a:qqeq69uvrh"),
            (Address::new(Prefix::A, Version::ScriptHash, b""), "a:pq99546ray"),
            (Address::new(Prefix::B, Version::ScriptHash, b" "), "b:pqsqzsjd64fv"),
            (Address::new(Prefix::B, Version::ScriptHash, b"-"), "b:pqksmhczf8ud"),
            (Address::new(Prefix::B, Version::ScriptHash, b"0"), "b:pqcq53eqrk0e"),
            (Address::new(Prefix::B, Version::ScriptHash, b"1"), "b:pqcshg75y0vf"),
            (Address::new(Prefix::B, Version::ScriptHash, b"-1"), "b:pqknzl4e9y0zy"),
            (Address::new(Prefix::B, Version::ScriptHash, b"11"), "b:pqcnzt888ytdg"),
            (Address::new(Prefix::B, Version::ScriptHash, b"abc"), "b:ppskycc8txxxn2w"),
            (Address::new(Prefix::B, Version::ScriptHash, b"1234598760"), "b:pqcnyve5x5unsdekxqeusxeyu2"),
            (Address::new(Prefix::B, Version::ScriptHash, b"abcdefghijklmnopqrstuvwxyz"), "b:ppskycmyv4nxw6rfdf4kcmtwdac8zunnw36hvamc09aqtpppz8lk"),
            (Address::new(Prefix::B, Version::ScriptHash, b"000000000000000000000000000000000000000000"), "b:pqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrq7ag684l3"),
            (Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]),      "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqhqrxplya"),
            (Address::new(Prefix::Testnet, Version::PubKeyECDSA, &[0u8; 33]), "kaspatest:qyqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqhe837j2d"),
            (Address::new(Prefix::Testnet, Version::PubKeyECDSA, b"\xba\x01\xfc\x5f\x4e\x9d\x98\x79\x59\x9c\x69\xa3\xda\xfd\xb8\x35\xa7\x25\x5e\x5f\x2e\x93\x4e\x93\x22\xec\xd3\xaf\x19\x0a\xb0\xf6\x0e"), "kaspatest:qxaqrlzlf6wes72en3568khahq66wf27tuhfxn5nytkd8tcep2c0vrse6gdmpks"),
            (Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]),      "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e"),
            (Address::new(Prefix::Mainnet, Version::PubKey, b"\x5f\xff\x3c\x4d\xa1\x8f\x45\xad\xcd\xd4\x99\xe4\x46\x11\xe9\xff\xf1\x48\xba\x69\xdb\x3c\x4e\xa2\xdd\xd9\x55\xfc\x46\xa5\x95\x22"), "kaspa:qp0l70zd5x85ttwd6jv7g3s3a8llzj96d8dncn4zmhv4tlzx5k2jyqh70xmfj"),
        ]
        // cspell:enable
    }

    #[test]
    fn check_into_string() {
        for (address, expected_address_str) in cases() {
            let address_str: String = address.into();
            assert_eq!(address_str, expected_address_str);
        }
    }

    #[test]
    fn check_from_string() {
        for (expected_address, address_str) in cases() {
            let address: Address = address_str.to_string().try_into().expect("Test failed");
            assert_eq!(address, expected_address);
        }
    }

    #[test]
    fn test_errors() {
        // cspell:disable
        let address_str: String = "kaspa:qqqqqqqqqqqqq1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::DecodingError('1')), address);

        let address_str: String = "kaspa1:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::InvalidPrefix("kaspa1".into())), address);

        let address_str: String = "kaspaqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::MissingPrefix), address);

        let address_str: String = "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4l".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::BadChecksum), address);

        let address_str: String = "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let address: Result<Address, AddressError> = address_str.try_into();
        assert_eq!(Err(AddressError::BadChecksum), address);
        // cspell:enable
    }
}
