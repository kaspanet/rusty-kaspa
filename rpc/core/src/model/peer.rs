use std::{
    net::{AddrParseError, SocketAddr},
    str::FromStr,
};

use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use kaspa_utils::{ip_address::IpAddress, net_address::NetAddress, peer_id::PeerId};
use serde::{Deserialize, Serialize};

pub type RpcNodeId = PeerId;
pub type RpcIpAddress = IpAddress;
pub type RpcPeerAddress = NetAddress;

/// A peer address possibly without explicit port
///
/// Use `normalize` to get a fully determined address.
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct RpcContextualPeerAddress {
    ip: IpAddress,
    port: Option<u16>,
}

impl RpcContextualPeerAddress {
    fn new(ip: IpAddress, port: Option<u16>) -> Self {
        Self { ip, port }
    }

    pub fn normalize(&self, default_port: u16) -> RpcPeerAddress {
        RpcPeerAddress::new(self.ip, self.port.unwrap_or(default_port))
    }
}

impl From<RpcPeerAddress> for RpcContextualPeerAddress {
    fn from(value: RpcPeerAddress) -> Self {
        Self::new(value.ip, Some(value.port))
    }
}

impl FromStr for RpcContextualPeerAddress {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match SocketAddr::from_str(s) {
            Ok(socket) => Ok(Self::new(socket.ip().into(), Some(socket.port()))),
            Err(_) => Ok(Self::new(IpAddress::from_str(s)?, None)),
        }
    }
}

impl ToString for RpcContextualPeerAddress {
    fn to_string(&self) -> String {
        match self.port {
            Some(port) => SocketAddr::new(self.ip.into(), port).to_string(),
            None => self.ip.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct RpcPeerInfo {
    pub id: RpcNodeId,
    pub address: RpcPeerAddress,
    pub last_ping_duration: u64, // NOTE: i64 in gRPC protowire

    pub is_outbound: bool,
    pub time_offset: i64,
    pub user_agent: String,

    pub advertised_protocol_version: u32,
    pub time_connected: u64, // NOTE: i64 in gRPC protowire
    pub is_ibd_peer: bool,
}
