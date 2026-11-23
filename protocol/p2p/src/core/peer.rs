use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_utils::{
    connection::{Libp2pId, PeerAddress},
    networking::{IpAddress, PeerId},
};
use std::{fmt::Display, net::SocketAddr, sync::Arc, time::Instant};

#[derive(Debug, Clone, Default)]
pub struct PeerProperties {
    /// Unchanged address advertisement received in the Kaspa handshake.
    pub address: Option<PeerAddress>,
    pub user_agent: String,
    // TODO: add services
    pub advertised_protocol_version: u32,
    pub protocol_version: u32,
    pub disable_relay_tx: bool,
    pub subnetwork_id: Option<SubnetworkId>,
    pub time_offset: i64,
}

#[derive(Debug)]
pub struct Peer {
    identity: PeerId,
    net_address: SocketAddr,
    /// Identity authenticated by libp2p, independent of the live socket and Kaspa UUID.
    libp2p_id: Option<Libp2pId>,
    is_outbound: bool,
    connection_started: Instant,
    properties: Arc<PeerProperties>,
    last_ping_duration: u64,
}

impl Peer {
    pub fn new(
        identity: PeerId,
        net_address: SocketAddr,
        libp2p_id: Option<Libp2pId>,
        is_outbound: bool,
        connection_started: Instant,
        properties: Arc<PeerProperties>,
        last_ping_duration: u64,
    ) -> Self {
        Self { identity, net_address, libp2p_id, is_outbound, connection_started, properties, last_ping_duration }
    }

    /// Internal identity of this peer
    pub fn identity(&self) -> PeerId {
        self.identity
    }

    /// The verified socket address of this connection.
    pub fn net_address(&self) -> SocketAddr {
        self.net_address
    }

    /// Authenticated libp2p identity, absent for TCP connections.
    pub fn libp2p_id(&self) -> Option<Libp2pId> {
        self.libp2p_id
    }

    pub fn key(&self) -> PeerKey {
        self.into()
    }

    /// Indicates whether this connection is an outbound connection
    pub fn is_outbound(&self) -> bool {
        self.is_outbound
    }

    pub fn time_connected(&self) -> u64 {
        Instant::now().duration_since(self.connection_started).as_millis() as u64
    }

    pub fn properties(&self) -> Arc<PeerProperties> {
        self.properties.clone()
    }

    pub fn last_ping_duration(&self) -> u64 {
        self.last_ping_duration
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum PeerKey {
    Tcp { identity: PeerId, ip: IpAddress },
    Libp2p(Libp2pId),
}

impl PeerKey {
    pub fn new(identity: PeerId, ip: IpAddress) -> Self {
        Self::Tcp { identity, ip }
    }
}

impl From<&Peer> for PeerKey {
    fn from(value: &Peer) -> Self {
        value.libp2p_id.map_or_else(|| Self::new(value.identity, value.net_address.ip().into()), Self::Libp2p)
    }
}

impl Display for PeerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp { identity, ip } => write!(f, "{identity}+{ip}"),
            Self::Libp2p(id) => write!(f, "libp2p://{}", hex::encode(id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn authenticated_identity_is_independent_of_socket_and_handshake_uuid() {
        let make_peer = |socket, id| Peer::new(PeerId::default(), socket, id, true, Instant::now(), Default::default(), 0);
        let socket: SocketAddr = "192.0.2.2:50000".parse().unwrap();
        let first = make_peer(socket, Some([1; 32]));
        let mut moved = make_peer("192.0.2.3:60000".parse().unwrap(), Some([1; 32]));
        moved.identity = PeerId::from_slice(&[1; 16]).unwrap();
        assert_ne!(first.identity(), moved.identity());
        assert_eq!(first.net_address(), socket);
        assert_eq!(first.key(), moved.key());
        assert_eq!(first.key(), PeerKey::Libp2p([1; 32]));
        let other = make_peer(socket, Some([2; 32]));
        let tcp = make_peer(socket, None);
        let peers = HashMap::from([(first.key(), &first), (other.key(), &other), (tcp.key(), &tcp)]);
        assert_eq!(peers.len(), 3, "sharing a socket must not merge distinct identities or transports");
        assert_eq!(tcp.key(), PeerKey::new(PeerId::default(), socket.ip().into()));
        assert_ne!(tcp.key(), PeerKey::new(moved.identity(), socket.ip().into()));
        assert_ne!(tcp.key(), PeerKey::new(tcp.identity(), "192.0.2.3".parse().unwrap()));
    }
}
