use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

// use net_address::{BorshDeserialize, BorshSchema, BorshSerialize};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

pub(super) mod address_store;
pub(super) mod banned_address_store;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug)]
pub struct NetAddress {
    pub ip: IpAddr,
    pub port: u16,
}

impl NetAddress {
    pub fn new(ip: IpAddr, port: u16) -> Self {
        Self { ip, port }
    }
}

impl From<SocketAddr> for NetAddress {
    fn from(value: SocketAddr) -> Self {
        Self::new(value.ip(), value.port())
    }
}

//
// Borsh serializers need to be manually implemented for `NetAddress` since
// IpAddr does not currently support Borsh
//

impl BorshSerialize for NetAddress {
    fn serialize<W: borsh::maybestd::io::Write>(&self, writer: &mut W) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
        let variant_idx: u8 = match self.ip {
            IpAddr::V4(..) => 0u8,
            IpAddr::V6(..) => 1u8,
        };
        writer.write_all(&variant_idx.to_le_bytes())?;
        match self.ip {
            IpAddr::V4(id0) => {
                borsh::BorshSerialize::serialize(&id0.octets(), writer)?;
            }
            IpAddr::V6(id0) => {
                borsh::BorshSerialize::serialize(&id0.octets(), writer)?;
            }
        }
        borsh::BorshSerialize::serialize(&self.port, writer)?;
        Ok(())
    }
}

impl borsh::de::BorshDeserialize for NetAddress {
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
        Ok(Self { ip, port: BorshDeserialize::deserialize(buf)? })
    }
}

impl BorshSchema for NetAddress {
    fn declaration() -> borsh::schema::Declaration {
        "NetAddress".to_string()
    }
    fn add_definitions_recursively(
        definitions: &mut borsh::maybestd::collections::HashMap<borsh::schema::Declaration, borsh::schema::Definition>,
    ) {
        #[allow(dead_code)]
        #[derive(BorshSchema)]
        enum IpAddr {
            V4([u8; 4]),
            V6([u8; 16]),
        }

        let fields = borsh::schema::Fields::NamedFields(borsh::maybestd::vec![
            ("ip".to_string(), <IpAddr>::declaration()),
            ("port".to_string(), <u16>::declaration())
        ]);
        let definition = borsh::schema::Definition::Struct { fields };
        Self::add_definition(Self::declaration(), definition, definitions);
        <IpAddr>::add_definitions_recursively(definitions);
        <u16>::add_definitions_recursively(definitions);
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct AddressKey(Ipv6Addr, u16);

impl AddressKey {
    pub fn new(ip: Ipv6Addr, port: u16) -> Self {
        Self(ip, port)
    }

    pub fn is_ip(&self, ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ip) => ip.to_ipv6_mapped() == self.0,
            IpAddr::V6(ip) => ip == self.0,
        }
    }
}

impl From<NetAddress> for AddressKey {
    fn from(value: NetAddress) -> Self {
        AddressKey::new(
            match value.ip {
                IpAddr::V4(ip) => ip.to_ipv6_mapped(),
                IpAddr::V6(ip) => ip,
            },
            value.port,
        )
    }
}
