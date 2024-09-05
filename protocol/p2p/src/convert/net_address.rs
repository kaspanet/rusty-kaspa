use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::error::ConversionError;
use crate::pb as protowire;

use itertools::Itertools;
use kaspa_utils::networking::{IpAddress, NetAddress};

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<(IpAddress, u16)> for protowire::NetAddress {
    fn from((ip, port): (IpAddress, u16)) -> Self {
        Self {
            timestamp: 0, // This field is not used anymore
            ip: match ip.0 {
                // We follow the IP encoding of golang's net.IP type
                IpAddr::V4(ip) => ip.octets().to_vec(),
                IpAddr::V6(ip) => ip.octets().to_vec(),
            },
            port: port as u32,
        }
    }
}

impl From<NetAddress> for protowire::NetAddress {
    fn from(item: NetAddress) -> Self {
        (item.ip, item.port).into()
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::NetAddress> for (IpAddress, u16) {
    type Error = ConversionError;

    fn try_from(addr: protowire::NetAddress) -> Result<Self, Self::Error> {
        // We follow the IP encoding of golang's net.IP type
        let ip: IpAddress = match addr.ip.len() {
            4 => Ok(Ipv4Addr::new(addr.ip[0], addr.ip[1], addr.ip[2], addr.ip[3]).into()),
            16 => {
                let octets = addr
                    .ip
                    .chunks(size_of::<u16>())
                    .map(|chunk| u16::from_be_bytes(chunk.try_into().expect("We already checked the number of bytes")))
                    .collect_vec();
                let ipv6 = Ipv6Addr::from(<[u16; 8]>::try_from(octets).unwrap());
                Ok(ipv6.into())
            }
            len => Err(ConversionError::IllegalIPLength(len)),
        }?;
        Ok((ip, addr.port.try_into()?))
    }
}

impl TryFrom<protowire::NetAddress> for NetAddress {
    type Error = ConversionError;

    fn try_from(item: protowire::NetAddress) -> Result<Self, Self::Error> {
        let (ip, port) = item.try_into()?;
        Ok(NetAddress::new(ip, port))
    }
}

#[cfg(test)]
mod tests {
    use kaspa_utils::networking::IpAddress;

    use crate::pb;
    use std::{
        net::{Ipv4Addr, Ipv6Addr},
        str::FromStr,
    };

    #[test]
    fn test_netaddress() {
        let net_addr_ipv4 = pb::NetAddress { timestamp: 0, ip: hex::decode("6a0a8af0").unwrap(), port: 123 };
        let ipv4 = Ipv4Addr::from_str("106.10.138.240").unwrap().into();
        assert_eq!(<(IpAddress, u16)>::try_from(net_addr_ipv4.clone()).unwrap(), (ipv4, 123u16));
        assert_eq!(pb::NetAddress::from((ipv4, 123u16)), net_addr_ipv4);

        let net_addr_ipv6 = pb::NetAddress { timestamp: 0, ip: hex::decode("20010db885a3000000008a2e03707334").unwrap(), port: 456 };
        let ipv6 = Ipv6Addr::from_str("2001:0db8:85a3:0000:0000:8a2e:0370:7334").unwrap().into();
        assert_eq!(<(IpAddress, u16)>::try_from(net_addr_ipv6.clone()).unwrap(), (ipv6, 456u16));
        assert_eq!(pb::NetAddress::from((ipv6, 456u16)), net_addr_ipv6);
    }
}
