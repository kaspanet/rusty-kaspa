use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::error::ConversionError;
use crate::pb::{self as protowire, net_address::Address as ProtoAddress};

use kaspa_utils::networking::{AddressKind, IpAddress, NetAddress, OnionAddress};

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<NetAddress> for protowire::NetAddress {
    fn from(item: NetAddress) -> Self {
        let address = match item.kind() {
            AddressKind::Ip(ip) => Some(ProtoAddress::Ip(match IpAddr::from(ip) {
                IpAddr::V4(v4) => v4.octets().to_vec(),
                IpAddr::V6(v6) => v6.octets().to_vec(),
            })),
            AddressKind::Onion(onion) => Some(ProtoAddress::Onion(onion.to_string())),
        };
        Self { timestamp: 0, address, port: item.port as u32 }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------
impl TryFrom<protowire::NetAddress> for NetAddress {
    type Error = ConversionError;

    fn try_from(item: protowire::NetAddress) -> Result<Self, Self::Error> {
        let port: u16 = item.port.try_into()?;
        let address = item.address.ok_or(ConversionError::NoneValue)?;
        match address {
            ProtoAddress::Ip(bytes) => {
                let ip = match bytes.len() {
                    4 => IpAddress::from(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3])),
                    16 => {
                        let mut arr = [0u8; 16];
                        arr.copy_from_slice(&bytes);
                        IpAddress::from(Ipv6Addr::from(arr))
                    }
                    len => return Err(ConversionError::IllegalIPLength(len)),
                };
                Ok(NetAddress::new(ip, port))
            }
            ProtoAddress::Onion(value) => {
                let onion = OnionAddress::try_from(value.as_str())?;
                Ok(NetAddress::new_onion(onion, port))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use kaspa_utils::networking::{NetAddress, OnionAddress};

    use crate::pb;
    use std::{
        net::{Ipv4Addr, Ipv6Addr},
        str::FromStr,
    };

    #[test]
    fn test_netaddress() {
        let net_addr_ipv4 =
            pb::NetAddress { timestamp: 0, address: Some(pb::net_address::Address::Ip(hex::decode("6a0a8af0").unwrap())), port: 123 };
        let ipv4 = Ipv4Addr::from_str("106.10.138.240").unwrap().into();
        assert_eq!(NetAddress::try_from(net_addr_ipv4.clone()).unwrap(), NetAddress::new(ipv4, 123u16));
        assert_eq!(pb::NetAddress::from(NetAddress::new(ipv4, 123u16)), net_addr_ipv4);

        let net_addr_ipv6 = pb::NetAddress {
            timestamp: 0,
            address: Some(pb::net_address::Address::Ip(hex::decode("20010db885a3000000008a2e03707334").unwrap())),
            port: 456,
        };
        let ipv6 = Ipv6Addr::from_str("2001:0db8:85a3:0000:0000:8a2e:0370:7334").unwrap().into();
        assert_eq!(NetAddress::try_from(net_addr_ipv6.clone()).unwrap(), NetAddress::new(ipv6, 456u16));
        assert_eq!(pb::NetAddress::from(NetAddress::new(ipv6, 456u16)), net_addr_ipv6);

        let onion = NetAddress::new_onion(
            OnionAddress::try_from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.onion").unwrap(),
            9000,
        );
        let proto = pb::NetAddress::from(onion);
        assert!(matches!(proto.address, Some(pb::net_address::Address::Onion(_))));
        assert_eq!(NetAddress::try_from(proto).unwrap(), onion);
    }
}
