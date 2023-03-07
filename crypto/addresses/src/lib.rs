use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

mod bech32;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum AddressError {
    InvalidPrefix(String),
    MissingPrefix,
    InvalidVersion(u8),
    DecodingError(char),
    BadChecksum,
}

impl Display for AddressError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Address decoding failed: {}",
            match self {
                Self::InvalidPrefix(prefix) => format!("Invalid prefix {prefix}"),
                Self::MissingPrefix => "Prefix is missing".to_string(),
                Self::InvalidVersion(version) => format!("Invalid version {version}"),
                Self::BadChecksum => "Checksum is invalid".to_string(),
                Self::DecodingError(c) => format!("Invalid character {c}"),
            }
        )
    }
}

impl std::error::Error for AddressError {}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Prefix {
    Mainnet,
    Testnet,
    Simnet,
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

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[repr(u8)]
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

#[derive(PartialEq, Eq, Clone, Debug, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Address {
    // TODO: consider using a smallvec for the payload
    pub prefix: Prefix,
    pub version: Version,
    pub payload: Vec<u8>,
}

impl Address {
    pub fn new(prefix: Prefix, version: Version, payload: &[u8]) -> Self {
        if !prefix.is_test() {
            assert_eq!(payload.len(), version.public_key_len());
        }
        Self { prefix, payload: payload.to_vec(), version }
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

#[cfg(test)]
mod tests {
    use crate::*;

    fn cases() -> Vec<(Address, &'static str)> {
        vec![
            (Address{prefix: Prefix::A, version: Version::PubKey,     payload: b"".to_vec()}, "a:qqeq69uvrh"),
            (Address{prefix: Prefix::A, version: Version::ScriptHash, payload: b"".to_vec()}, "a:pq99546ray"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b" ".to_vec()}, "b:pqsqzsjd64fv"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b"-".to_vec()}, "b:pqksmhczf8ud"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b"0".to_vec()}, "b:pqcq53eqrk0e"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b"1".to_vec()}, "b:pqcshg75y0vf"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b"-1".to_vec()}, "b:pqknzl4e9y0zy"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b"11".to_vec()}, "b:pqcnzt888ytdg"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b"abc".to_vec()}, "b:ppskycc8txxxn2w"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b"1234598760".to_vec()}, "b:pqcnyve5x5unsdekxqeusxeyu2"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b"abcdefghijklmnopqrstuvwxyz".to_vec()}, "b:ppskycmyv4nxw6rfdf4kcmtwdac8zunnw36hvamc09aqtpppz8lk"),
            (Address{prefix: Prefix::B, version: Version::ScriptHash, payload: b"000000000000000000000000000000000000000000".to_vec()}, "b:pqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrq7ag684l3"),
            (Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]),      "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqhqrxplya"),
            (Address::new(Prefix::Testnet, Version::PubKeyECDSA, &[0u8; 33]), "kaspatest:qyqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqhe837j2d"),
            (Address::new(Prefix::Testnet, Version::PubKeyECDSA, b"\xba\x01\xfc\x5f\x4e\x9d\x98\x79\x59\x9c\x69\xa3\xda\xfd\xb8\x35\xa7\x25\x5e\x5f\x2e\x93\x4e\x93\x22\xec\xd3\xaf\x19\x0a\xb0\xf6\x0e"), "kaspatest:qxaqrlzlf6wes72en3568khahq66wf27tuhfxn5nytkd8tcep2c0vrse6gdmpks"),
            (Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]),      "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e"),
            (Address::new(Prefix::Mainnet, Version::PubKey, b"\x5f\xff\x3c\x4d\xa1\x8f\x45\xad\xcd\xd4\x99\xe4\x46\x11\xe9\xff\xf1\x48\xba\x69\xdb\x3c\x4e\xa2\xdd\xd9\x55\xfc\x46\xa5\x95\x22"), "kaspa:qp0l70zd5x85ttwd6jv7g3s3a8llzj96d8dncn4zmhv4tlzx5k2jyqh70xmfj"),
        ]
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
    }
}
