use std::fmt::{Display, Formatter};

mod bech32;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum AddressError {
    InvalidPrefix(String),
    MissingPrefix,
    DecodingError(char),
    BadChecksum,
}

impl Display for AddressError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Address decoding failed: {}",
            match self {
                Self::InvalidPrefix(prefix) => format!("Invalid prefix {}", prefix),
                Self::MissingPrefix => "Prefix is missing".to_string(),
                Self::BadChecksum => "Checksum is invalid".to_string(),
                Self::DecodingError(c) => format!("Invalid character {}", c),
            }
        )
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Prefix {
    Mainnet,
    Testnet,
    Devnet,
}

impl Display for Prefix {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Prefix::Mainnet => "kaspa",
            Prefix::Testnet => "kaspatest",
            Prefix::Devnet => "kaspadev",
        })
    }
}

impl TryFrom<&str> for Prefix {
    type Error = AddressError;

    fn try_from(prefix: &str) -> Result<Self, Self::Error> {
        match prefix {
            "kaspa" => Ok(Prefix::Mainnet),
            "kaspatest" => Ok(Prefix::Testnet),
            "kaspadev" => Ok(Prefix::Devnet),
            _ => Err(AddressError::InvalidPrefix(prefix.to_string())),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Address {
    prefix: Prefix,
    payload: Vec<u8>,
    version: u8,
}

impl From<Address> for String {
    fn from(address: Address) -> Self {
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

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn check_into_string() {
        let address = Address { prefix: Prefix::Mainnet, payload: vec![0u8; 32], version: 0u8 };
        let address_str: String = address.into();
        assert_eq!("kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e", address_str);

        let address = Address {
            prefix: Prefix::Mainnet,
            payload: b"\x5f\xff\x3c\x4d\xa1\x8f\x45\xad\xcd\xd4\x99\xe4\x46\x11\xe9\xff\xf1\x48\xba\x69\xdb\x3c\x4e\xa2\xdd\xd9\x55\xfc\x46\xa5\x95\x22".to_vec(),
            version: 0u8
        };
        let address_str: String = address.into();
        assert_eq!("kaspa:qp0l70zd5x85ttwd6jv7g3s3a8llzj96d8dncn4zmhv4tlzx5k2jyqh70xmfj", address_str);
    }

    #[test]
    fn check_from_string() {
        let address_str: String = "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e".to_string();
        let address: Address = address_str.try_into().expect("Test failed");
        assert_eq!(Address { prefix: Prefix::Mainnet, payload: vec![0u8; 32], version: 0u8 }, address);

        let address_str: String = "kaspa:qp0l70zd5x85ttwd6jv7g3s3a8llzj96d8dncn4zmhv4tlzx5k2jyqh70xmfj".to_string();
        let address = address_str.try_into().expect("Test failed");
        assert_eq!(Address {
            prefix: Prefix::Mainnet,
            payload: b"\x5f\xff\x3c\x4d\xa1\x8f\x45\xad\xcd\xd4\x99\xe4\x46\x11\xe9\xff\xf1\x48\xba\x69\xdb\x3c\x4e\xa2\xdd\xd9\x55\xfc\x46\xa5\x95\x22".to_vec(),
            version: 0u8
        }, address);
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
