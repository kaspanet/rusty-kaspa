use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    net::{AddrParseError, IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    ops::Deref,
    str::FromStr,
};
use uuid::Uuid;

/// An IP address, newtype of [IpAddr].
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug)]
#[repr(transparent)]
pub struct IpAddress(pub IpAddr);

impl IpAddress {
    pub fn new(ip: IpAddr) -> Self {
        Self(ip)
    }

    pub fn is_publicly_routable(&self) -> bool {
        if self.is_loopback() || self.is_unspecified() {
            return false;
        }

        // Based on values from network.go
        let unroutable_nets = [
            "198.18.0.0/15",   // RFC 2544
            "2001:DB8::/32",   // RFC 3849
            "10.0.0.0/8",      // RFC 1918
            "172.16.0.0/12",   // RFC 1918
            "192.168.0.0/16",  // RFC 1918
            "169.254.0.0/16",  // RFC 3927
            "2002::/16",       // RFC 3964
            "FC00::/7",        // RFC 4193
            "2001::/32",       // RFC 4380
            "2001:10::/28",    // RFC 4843
            "FE80::/64",       // RFC 4862
            "192.0.2.0/24",    // RFC 5737
            "198.51.100.0/24", // RFC 5737
            "203.0.113.0/24",  // RFC 5737
            "64:FF9B::/96",    // RFC 6052
            "::FFFF:0:0:0/96", // RFC 6145
            "100.64.0.0/10",   // RFC 6598
            "0.0.0.0/8",       // Zero Net
            "2001:470::/32",   // Hurricane Electric IPv6 address block.
        ];

        for curr_net in unroutable_nets {
            if IpNet::from_str(curr_net).unwrap().contains(&self.0) {
                return false;
            }
        }

        true
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

/// A network address, equivalent of a [SocketAddr].
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct NetAddress {
    pub ip: IpAddress,
    pub port: u16,
}

impl NetAddress {
    pub fn new(ip: IpAddress, port: u16) -> Self {
        Self { ip, port }
    }
}

impl From<SocketAddr> for NetAddress {
    fn from(value: SocketAddr) -> Self {
        Self::new(value.ip().into(), value.port())
    }
}

impl From<NetAddress> for SocketAddr {
    fn from(value: NetAddress) -> Self {
        Self::new(value.ip.0, value.port)
    }
}

impl FromStr for NetAddress {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SocketAddr::from_str(s).map(NetAddress::from)
    }
}

impl Display for NetAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        SocketAddr::from(self.to_owned()).fmt(f)
    }
}

/// A network address possibly without explicit port.
///
/// Use `normalize` to get a fully determined address.
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct ContextualNetAddress {
    ip: IpAddress,
    port: Option<u16>,
}

impl ContextualNetAddress {
    fn new(ip: IpAddress, port: Option<u16>) -> Self {
        Self { ip, port }
    }

    pub fn normalize(&self, default_port: u16) -> NetAddress {
        NetAddress::new(self.ip, self.port.unwrap_or(default_port))
    }

    pub fn unspecified() -> Self {
        Self { ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)).into(), port: None }
    }

    pub fn loopback() -> Self {
        Self { ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)).into(), port: None }
    }
}

impl From<NetAddress> for ContextualNetAddress {
    fn from(value: NetAddress) -> Self {
        Self::new(value.ip, Some(value.port))
    }
}

impl FromStr for ContextualNetAddress {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match SocketAddr::from_str(s) {
            Ok(socket) => Ok(Self::new(socket.ip().into(), Some(socket.port()))),
            Err(_) => Ok(Self::new(IpAddress::from_str(s)?, None)),
        }
    }
}

impl Display for ContextualNetAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.port {
            Some(port) => SocketAddr::new(self.ip.into(), port).fmt(f),
            None => self.ip.fmt(f),
        }
    }
}
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug, Default)]
#[repr(transparent)]
pub struct PeerId(pub Uuid);

impl PeerId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, uuid::Error> {
        Ok(Uuid::from_slice(bytes)?.into())
    }
}
impl From<Uuid> for PeerId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}
impl From<PeerId> for Uuid {
    fn from(value: PeerId) -> Self {
        value.0
    }
}

impl FromStr for PeerId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(PeerId::from)
    }
}

impl Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for PeerId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//
// Borsh serializers need to be manually implemented for `PeerId` since
// Uuid does not currently support Borsh
//

impl BorshSerialize for PeerId {
    fn serialize<W: borsh::maybestd::io::Write>(&self, writer: &mut W) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
        borsh::BorshSerialize::serialize(&self.0.as_bytes(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for PeerId {
    fn deserialize(buf: &mut &[u8]) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
        let bytes: uuid::Bytes = BorshDeserialize::deserialize(buf)?;
        Ok(Self::new(Uuid::from_bytes(bytes)))
    }
}

impl BorshSchema for PeerId {
    fn declaration() -> borsh::schema::Declaration {
        "PeerId".to_string()
    }
    fn add_definitions_recursively(
        definitions: &mut borsh::maybestd::collections::HashMap<borsh::schema::Declaration, borsh::schema::Definition>,
    ) {
        let fields = borsh::schema::Fields::UnnamedFields(borsh::maybestd::vec![<uuid::Bytes>::declaration()]);
        let definition = borsh::schema::Definition::Struct { fields };
        Self::add_definition(Self::declaration(), definition, definitions);
        <uuid::Bytes>::add_definitions_recursively(definitions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

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

    #[test]
    fn test_peer_id_borsh() {
        // Tests for PeerId Borsh ser/deser since we manually implemented them
        let id: PeerId = Uuid::new_v4().into();
        let bin = id.try_to_vec().unwrap();
        let id2: PeerId = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(id, id2);

        let id: PeerId = Uuid::from_bytes([123u8; 16]).into();
        let bin = id.try_to_vec().unwrap();
        let id2: PeerId = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(id, id2);
    }

    #[test]
    fn test_net_address_from_str() {
        let addr_v4 = NetAddress::from_str("1.2.3.4:5678");
        assert!(addr_v4.is_ok());
        let addr_v6 = NetAddress::from_str("[2a01:4f8:191:1143::2]:5678");
        assert!(addr_v6.is_ok());
    }

    #[test]
    fn test_is_publicly_routable() {
        // RFC 2544 tests
        assert_eq!(false, IpAddress::from_str("198.18.0.0").unwrap().is_publicly_routable());
        assert_eq!(false, IpAddress::from_str("198.19.255.255").unwrap().is_publicly_routable());
        assert_eq!(true, IpAddress::from_str("198.17.255.255").unwrap().is_publicly_routable());
        assert_eq!(true, IpAddress::from_str("198.20.0.0").unwrap().is_publicly_routable());

        // Zero net tests
        assert_eq!(false, IpAddress::from_str("0.0.0.0").unwrap().is_publicly_routable());
        assert_eq!(false, IpAddress::from_str("0.0.0.1").unwrap().is_publicly_routable());
        assert_eq!(false, IpAddress::from_str("0.0.1.0").unwrap().is_publicly_routable());
        assert_eq!(false, IpAddress::from_str("0.1.0.0").unwrap().is_publicly_routable());

        // Localhost
        assert_eq!(false, IpAddress::from_str("127.0.0.1").unwrap().is_publicly_routable());

        // Some random routable IP
        assert_eq!(true, IpAddress::from_str("123.45.67.89").unwrap().is_publicly_routable());

        // TODO: Write more unit tests for the rest of the ranges
    }
}
