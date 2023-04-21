use std::{
    net::{AddrParseError, SocketAddr},
    str::FromStr,
};

use crate::ip_address::IpAddress;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

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

impl ToString for NetAddress {
    fn to_string(&self) -> String {
        SocketAddr::from(self.to_owned()).to_string()
    }
}

impl FromStr for NetAddress {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SocketAddr::from_str(s).map(NetAddress::from)
    }
}

#[cfg(test)]
mod tests {
    use crate::net_address::NetAddress;
    use std::str::FromStr;

    #[test]
    fn test_net_address_from_str() {
        let addr_v4 = NetAddress::from_str("1.2.3.4:5678");
        assert!(addr_v4.is_ok());
        let addr_v6 = NetAddress::from_str("[2a01:4f8:191:1143::2]:5678");
        assert!(addr_v6.is_ok());
    }
}
