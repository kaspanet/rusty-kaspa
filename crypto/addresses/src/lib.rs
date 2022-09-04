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
            Prefix::Devnet => "kaspadev",
            #[cfg(test)]
            Prefix::A => "a",
            #[cfg(test)]
            Prefix::B => "b",
        }
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
            "kaspadev" => Ok(Prefix::Devnet),
            #[cfg(test)]
            "a" => Ok(Prefix::A),
            #[cfg(test)]
            "b" => Ok(Prefix::B),
            _ => Err(AddressError::InvalidPrefix(prefix.to_string())),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Address {
    pub prefix: Prefix,
    pub payload: Vec<u8>,
    pub version: u8,
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

    fn cases() -> Vec<(Address, &'static str)> {
        vec![
            (Address{prefix: Prefix::A, version: 0, payload: b"".to_vec()}, "a:qqeq69uvrh"),
            (Address{prefix: Prefix::A, version: 8, payload: b"".to_vec()}, "a:pq99546ray"),
            (Address{prefix: Prefix::A, version: 120, payload: b"".to_vec()}, "a:0qf6jrhtdq"),
            (Address{prefix: Prefix::B, version: 8, payload: b" ".to_vec()}, "b:pqsqzsjd64fv"),
            (Address{prefix: Prefix::B, version: 8, payload: b"-".to_vec()}, "b:pqksmhczf8ud"),
            (Address{prefix: Prefix::B, version: 8, payload: b"0".to_vec()}, "b:pqcq53eqrk0e"),
            (Address{prefix: Prefix::B, version: 8, payload: b"1".to_vec()}, "b:pqcshg75y0vf"),
            (Address{prefix: Prefix::B, version: 8, payload: b"-1".to_vec()}, "b:pqknzl4e9y0zy"),
            (Address{prefix: Prefix::B, version: 8, payload: b"11".to_vec()}, "b:pqcnzt888ytdg"),
            (Address{prefix: Prefix::B, version: 8, payload: b"abc".to_vec()}, "b:ppskycc8txxxn2w"),
            (Address{prefix: Prefix::B, version: 8, payload: b"1234598760".to_vec()}, "b:pqcnyve5x5unsdekxqeusxeyu2"),
            (Address{prefix: Prefix::B, version: 8, payload: b"abcdefghijklmnopqrstuvwxyz".to_vec()}, "b:ppskycmyv4nxw6rfdf4kcmtwdac8zunnw36hvamc09aqtpppz8lk"),
            (Address{prefix: Prefix::B, version: 8, payload: b"000000000000000000000000000000000000000000".to_vec()}, "b:pqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrqvpsxqcrq7ag684l3"),
            (Address { prefix: Prefix::Mainnet, payload: vec![0u8; 32], version: 0u8 }, "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqkx9awp4e"),
            (Address { prefix: Prefix::Mainnet, payload: b"\x5f\xff\x3c\x4d\xa1\x8f\x45\xad\xcd\xd4\x99\xe4\x46\x11\xe9\xff\xf1\x48\xba\x69\xdb\x3c\x4e\xa2\xdd\xd9\x55\xfc\x46\xa5\x95\x22".to_vec(), version: 0u8 }, "kaspa:qp0l70zd5x85ttwd6jv7g3s3a8llzj96d8dncn4zmhv4tlzx5k2jyqh70xmfj"),
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
            let address: Address = address_str
                .to_string()
                .try_into()
                .expect("Test failed");
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
