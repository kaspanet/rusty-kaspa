use kaspa_utils::{ip_address::IpAddress, peer_id::PeerId};
use std::{fmt::Display, net::SocketAddr};

use crate::Router;

#[derive(Debug)]
pub struct Peer {
    identity: PeerId,
    net_address: SocketAddr,
    is_outbound: bool,
}

impl Peer {
    pub fn new(identity: PeerId, net_address: SocketAddr, is_outbound: bool) -> Self {
        Self { identity, net_address, is_outbound }
    }

    /// Internal identity of this peer
    pub fn identity(&self) -> PeerId {
        self.identity
    }

    /// The socket address of this peer
    pub fn net_address(&self) -> SocketAddr {
        self.net_address
    }

    pub fn key(&self) -> PeerKey {
        self.into()
    }

    /// Indicates whether this connection is an outbound connection
    pub fn is_outbound(&self) -> bool {
        self.is_outbound
    }
}

impl From<&Router> for Peer {
    fn from(router: &Router) -> Self {
        Self::new(router.identity(), router.net_address(), router.is_outbound())
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct PeerKey {
    identity: PeerId,
    ip: IpAddress,
}

impl PeerKey {
    pub fn new(identity: PeerId, ip: IpAddress) -> Self {
        Self { identity, ip }
    }
}

impl From<&Peer> for PeerKey {
    fn from(value: &Peer) -> Self {
        Self::new(value.identity, value.net_address.ip().into())
    }
}

impl Display for PeerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}+{}", self.identity, self.ip)
    }
}
