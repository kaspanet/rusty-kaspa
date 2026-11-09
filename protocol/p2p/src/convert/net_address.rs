use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::error::ConversionError;
use crate::libp2p_transport::Libp2pId;
use crate::pb as protowire;

use itertools::Itertools;
use kaspa_utils::{
    connection::{LibP2pAddress, Libp2pMetadata, PeerAddress},
    networking::{IpAddress, NetAddress},
};

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
            ..Default::default()
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
        let net_addr_ipv4 = pb::NetAddress { timestamp: 0, ip: hex::decode("6a0a8af0").unwrap(), port: 123, ..Default::default() };
        let ipv4 = Ipv4Addr::from_str("106.10.138.240").unwrap().into();
        assert_eq!(<(IpAddress, u16)>::try_from(net_addr_ipv4.clone()).unwrap(), (ipv4, 123u16));
        assert_eq!(pb::NetAddress::from((ipv4, 123u16)), net_addr_ipv4);

        let net_addr_ipv6 = pb::NetAddress {
            timestamp: 0,
            ip: hex::decode("20010db885a3000000008a2e03707334").unwrap(),
            port: 456,
            ..Default::default()
        };
        let ipv6 = Ipv6Addr::from_str("2001:0db8:85a3:0000:0000:8a2e:0370:7334").unwrap().into();
        assert_eq!(<(IpAddress, u16)>::try_from(net_addr_ipv6.clone()).unwrap(), (ipv6, 456u16));
        assert_eq!(pb::NetAddress::from((ipv6, 456u16)), net_addr_ipv6);
    }
}

impl From<std::net::SocketAddr> for protowire::SocketAddress {
    fn from(address: std::net::SocketAddr) -> Self {
        let ip = match address.ip() {
            IpAddr::V4(ip) => ip.octets().to_vec(),
            IpAddr::V6(ip) => ip.octets().to_vec(),
        };
        Self { ip, port: address.port().into() }
    }
}

impl TryFrom<protowire::SocketAddress> for std::net::SocketAddr {
    type Error = ConversionError;
    fn try_from(address: protowire::SocketAddress) -> Result<Self, Self::Error> {
        let ip = if let Ok(octets) = <[u8; 4]>::try_from(address.ip.as_slice()) {
            IpAddr::V4(Ipv4Addr::from(octets))
        } else if let Ok(octets) = <[u8; 16]>::try_from(address.ip.as_slice()) {
            IpAddr::V6(Ipv6Addr::from(octets))
        } else {
            return Err(ConversionError::Message(format!("Libp2p socket IP must contain 4 or 16 bytes, got {}", address.ip.len())));
        };
        let port = u16::try_from(address.port)
            .map_err(|_| ConversionError::Message(format!("Libp2p socket port exceeds 65535: {}", address.port)))?;
        Ok(Self::new(ip, port))
    }
}

impl From<LibP2pAddress> for protowire::LibP2pAddress {
    fn from(relay: LibP2pAddress) -> Self {
        Self { address: Some(relay.address.into()), endpoint_id: relay.id.to_vec() }
    }
}

impl TryFrom<protowire::LibP2pAddress> for LibP2pAddress {
    type Error = ConversionError;

    fn try_from(relay: protowire::LibP2pAddress) -> Result<Self, Self::Error> {
        let id = relay.endpoint_id.as_slice().try_into().map_err(|_| {
            ConversionError::Message(format!("Relay ID must contain {} bytes, got {}", size_of::<Libp2pId>(), relay.endpoint_id.len()))
        })?;
        let address = relay.address.ok_or_else(|| ConversionError::Message("Relay socket address is missing".into()))?.try_into()?;
        Ok(Self { address, id })
    }
}

impl From<Libp2pMetadata> for protowire::Libp2pMetadata {
    fn from(metadata: Libp2pMetadata) -> Self {
        Self {
            endpoint_id: metadata.endpoint_id.to_vec(),
            port: metadata.port.into(),
            relay_port: metadata.relay_port.into(),
            nonce: metadata.nonce,
            signature: metadata.signature,
        }
    }
}

impl TryFrom<protowire::Libp2pMetadata> for Libp2pMetadata {
    type Error = ConversionError;
    fn try_from(metadata: protowire::Libp2pMetadata) -> Result<Self, Self::Error> {
        fn port(value: u32) -> Result<u16, ConversionError> {
            value.try_into().map_err(|_| ConversionError::Message(format!("Advertised port exceeds 65535: {value}")))
        }
        let endpoint_id = metadata.endpoint_id.as_slice().try_into().map_err(|_| {
            ConversionError::Message(format!(
                "Endpoint ID must contain {} bytes, got {}",
                size_of::<Libp2pId>(),
                metadata.endpoint_id.len()
            ))
        })?;
        if !metadata.signature.is_empty() && metadata.signature.len() != 64 {
            return Err(ConversionError::Message(format!(
                "Address signature must contain 64 bytes, got {}",
                metadata.signature.len()
            )));
        }
        Ok(Self {
            endpoint_id,
            port: port(metadata.port)?,
            relay_port: port(metadata.relay_port)?,
            nonce: metadata.nonce,
            signature: metadata.signature,
        })
    }
}

impl From<PeerAddress> for protowire::NetAddress {
    fn from(address: PeerAddress) -> Self {
        let tcp = address.tcp_address();
        Self { libp2p: address.libp2p.map(Into::into), relay_address: address.relay_address.map(Into::into), ..tcp.into() }
    }
}

impl TryFrom<protowire::NetAddress> for PeerAddress {
    type Error = ConversionError;
    fn try_from(mut wire: protowire::NetAddress) -> Result<Self, Self::Error> {
        let libp2p = wire.libp2p.take().map(TryInto::try_into).transpose()?;
        let relay_address = wire.relay_address.take().map(TryInto::try_into).transpose()?;
        let (ip, tcp_port) =
            wire.try_into().map_err(|error| ConversionError::Message(format!("Invalid advertised TCP address: {error}")))?;
        Ok(Self { ip, tcp_port, libp2p, relay_address })
    }
}

#[cfg(test)]
mod signed_address_tests {
    use super::*;
    use prost::Message;

    #[test]
    fn legacy_wire_defaults_and_grouped_metadata_round_trip() {
        let tcp: NetAddress = "8.8.8.8:16111".parse().unwrap();
        let legacy = protowire::NetAddress::decode(&[0x1a, 4, 8, 8, 8, 8, 0x20, 0xef, 0x7d][..]).unwrap();
        assert_eq!(PeerAddress::try_from(legacy).unwrap(), PeerAddress::from(tcp));
        for relay_address in [None, Some(LibP2pAddress { address: "9.9.9.9:16112".parse().unwrap(), id: [3; 32] })] {
            let address = PeerAddress {
                libp2p: Some(Libp2pMetadata {
                    endpoint_id: [1; 32],
                    relay_port: 16112,
                    port: 30000,
                    nonce: 42,
                    signature: vec![2; 64],
                }),
                relay_address,
                ..tcp.into()
            };
            let wire = protowire::NetAddress::from(address.clone());
            assert_eq!(NetAddress::try_from(wire.clone()).unwrap(), tcp, "legacy readers still see TCP");
            let decoded = protowire::NetAddress::decode(wire.encode_to_vec().as_slice()).unwrap();
            assert_eq!(PeerAddress::try_from(decoded).unwrap(), address);
            let version = crate::convert::model::version::Version::new(
                Some(address.clone()),
                Default::default(),
                "kaspa-simnet".into(),
                None,
                11,
            );
            let wire: protowire::VersionMessage = version.into();
            assert_eq!(crate::convert::model::version::Version::try_from(wire).unwrap().address, Some(address));
        }
    }

    #[test]
    fn malformed_grouped_metadata_has_descriptive_conversion_errors() {
        let base = protowire::NetAddress::from("8.8.8.8:16111".parse::<NetAddress>().unwrap());
        let metadata = protowire::Libp2pMetadata { endpoint_id: vec![1; 32], signature: vec![2; 64], ..Default::default() };
        for libp2p in [
            protowire::Libp2pMetadata { endpoint_id: vec![1; 31], ..metadata.clone() },
            protowire::Libp2pMetadata { signature: vec![1; 65], ..metadata.clone() },
            protowire::Libp2pMetadata { relay_port: 65536, ..metadata.clone() },
            protowire::Libp2pMetadata { port: 65536, ..metadata },
        ] {
            assert!(matches!(
                PeerAddress::try_from(protowire::NetAddress { libp2p: Some(libp2p), ..base.clone() }),
                Err(ConversionError::Message(_))
            ));
        }
        let relay = protowire::LibP2pAddress {
            address: Some("9.9.9.9:16112".parse::<std::net::SocketAddr>().unwrap().into()),
            endpoint_id: vec![3; 32],
        };
        for relay_address in [
            protowire::LibP2pAddress { endpoint_id: vec![3; 31], ..relay.clone() },
            protowire::LibP2pAddress { endpoint_id: vec![3; 33], ..relay.clone() },
            protowire::LibP2pAddress { address: None, ..relay.clone() },
            protowire::LibP2pAddress { address: Some(protowire::SocketAddress { ip: vec![1; 3], port: 1 }), ..relay.clone() },
            protowire::LibP2pAddress { address: Some(protowire::SocketAddress { ip: vec![9; 4], port: 65536 }), ..relay },
        ] {
            let bad = protowire::NetAddress { relay_address: Some(relay_address), ..base.clone() };
            assert!(matches!(PeerAddress::try_from(bad), Err(ConversionError::Message(_))));
        }
    }
}
