use std::{
    fmt::Display,
    net::{AddrParseError, IpAddr, Ipv4Addr, Ipv6Addr},
    ops::Deref,
    str::FromStr,
};

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug)]
#[repr(transparent)]
pub struct IpAddress(pub IpAddr);

impl IpAddress {
    pub fn new(ip: IpAddr) -> Self {
        Self(ip)
    }
}
impl From<IpAddr> for IpAddress {
    fn from(ip: IpAddr) -> Self {
        Self(ip)
    }
}
impl From<Ipv4Addr> for IpAddress {
    fn from(value: Ipv4Addr) -> Self {
        Self(value.into())
    }
}
impl From<Ipv6Addr> for IpAddress {
    fn from(value: Ipv6Addr) -> Self {
        Self(value.into())
    }
}
impl From<IpAddress> for IpAddr {
    fn from(value: IpAddress) -> Self {
        value.0
    }
}

impl FromStr for IpAddress {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        IpAddr::from_str(s).map(IpAddress::from)
    }
}

impl Display for IpAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for IpAddress {
    type Target = IpAddr;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//
// Borsh serializers need to be manually implemented for `NetAddress` since
// IpAddr does not currently support Borsh
//

impl BorshSerialize for IpAddress {
    fn serialize<W: borsh::maybestd::io::Write>(&self, writer: &mut W) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
        let variant_idx: u8 = match self.0 {
            IpAddr::V4(..) => 0u8,
            IpAddr::V6(..) => 1u8,
        };
        writer.write_all(&variant_idx.to_le_bytes())?;
        match self.0 {
            IpAddr::V4(id0) => {
                borsh::BorshSerialize::serialize(&id0.octets(), writer)?;
            }
            IpAddr::V6(id0) => {
                borsh::BorshSerialize::serialize(&id0.octets(), writer)?;
            }
        }
        Ok(())
    }
}

impl BorshDeserialize for IpAddress {
    fn deserialize(buf: &mut &[u8]) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
        let variant_idx: u8 = BorshDeserialize::deserialize(buf)?;
        let ip = match variant_idx {
            0u8 => {
                let octets: [u8; 4] = BorshDeserialize::deserialize(buf)?;
                IpAddr::V4(Ipv4Addr::from(octets))
            }
            1u8 => {
                let octets: [u8; 16] = BorshDeserialize::deserialize(buf)?;
                IpAddr::V6(Ipv6Addr::from(octets))
            }
            _ => {
                let msg = borsh::maybestd::format!("Unexpected variant index: {:?}", variant_idx);
                return Err(borsh::maybestd::io::Error::new(borsh::maybestd::io::ErrorKind::InvalidInput, msg));
            }
        };
        Ok(Self(ip))
    }
}

impl BorshSchema for IpAddress {
    fn declaration() -> borsh::schema::Declaration {
        "IpAddress".to_string()
    }
    fn add_definitions_recursively(
        definitions: &mut borsh::maybestd::collections::HashMap<borsh::schema::Declaration, borsh::schema::Definition>,
    ) {
        #[allow(dead_code)]
        #[derive(BorshSchema)]
        enum IpAddress {
            V4([u8; 4]),
            V6([u8; 16]),
        }
        <IpAddress>::add_definitions_recursively(definitions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_address_borsh() {
        // Tests for IpAddress Borsh ser/deser since we manually implemented them
        let ip: IpAddress = Ipv4Addr::from([44u8; 4]).into();
        let bin = ip.try_to_vec().unwrap();
        let ip2: IpAddress = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(ip, ip2);

        let ip: IpAddress = Ipv6Addr::from([66u8; 16]).into();
        let bin = ip.try_to_vec().unwrap();
        let ip2: IpAddress = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(ip, ip2);
    }
}
