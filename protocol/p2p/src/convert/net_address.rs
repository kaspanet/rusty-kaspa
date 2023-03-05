use std::{
    mem::size_of,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use super::error::ConversionError;
use crate::pb as protowire;

use itertools::Itertools;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<(IpAddr, u16)> for protowire::NetAddress {
    fn from((ip, port): (IpAddr, u16)) -> Self {
        Self {
            timestamp: 0, // This field is not used anymore
            ip: match ip {
                IpAddr::V4(ip) => ip.octets().to_vec(), // TODO: Check endianness
                IpAddr::V6(ip) => ip.octets().to_vec(), // TODO: Check endianness
            },
            port: port as u32,
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::NetAddress> for (IpAddr, u16) {
    type Error = ConversionError;

    fn try_from(addr: protowire::NetAddress) -> Result<Self, Self::Error> {
        let ip: IpAddr = match addr.ip.len() {
            4 => Ok(Ipv4Addr::new(addr.ip[3], addr.ip[2], addr.ip[1], addr.ip[0]).into()),
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
