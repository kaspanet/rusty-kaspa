use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use serde::{Deserialize, Serialize};

pub(super) mod banned_address_store;
pub(super) mod not_banned_address_store;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize)]
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
