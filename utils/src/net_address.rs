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
