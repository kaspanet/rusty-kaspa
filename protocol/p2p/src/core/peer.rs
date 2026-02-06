use kaspa_consensus_core::{Hash as BlockHash, subnets::SubnetworkId};
use kaspa_utils::networking::{IpAddress, PeerId};
use std::{collections::HashMap, fmt::Display, hash::Hash, net::SocketAddr, sync::Arc, time::Instant};

#[derive(Copy, Debug, Clone)]
pub enum PeerOutboundType {
    Perigee,
    RandomGraph,
    UserSupplied,
}

impl Display for PeerOutboundType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerOutboundType::Perigee => write!(f, "perigee"),
            PeerOutboundType::RandomGraph => write!(f, "random graph"),
            PeerOutboundType::UserSupplied => write!(f, "user supplied"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PeerProperties {
    pub user_agent: String,
    // TODO: add services
    pub advertised_protocol_version: u32,
    pub protocol_version: u32,
    pub disable_relay_tx: bool,
    pub subnetwork_id: Option<SubnetworkId>,
    pub time_offset: i64,
}

#[derive(Debug, Clone)]
pub struct Peer {
    identity: PeerId,
    net_address: SocketAddr,
    outbound_type: Option<PeerOutboundType>,
    connection_started: Instant,
    properties: Arc<PeerProperties>,
    last_ping_duration: u64,
    perigee_timestamps: Arc<HashMap<BlockHash, Instant>>,
}

impl Peer {
    pub fn new(
        identity: PeerId,
        net_address: SocketAddr,
        outbound_type: Option<PeerOutboundType>,
        connection_started: Instant,
        properties: Arc<PeerProperties>,
        last_ping_duration: u64,
        perigee_timestamps: Arc<HashMap<BlockHash, Instant>>,
    ) -> Self {
        Self { identity, net_address, outbound_type, connection_started, properties, last_ping_duration, perigee_timestamps }
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

    pub fn outbound_type(&self) -> Option<PeerOutboundType> {
        self.outbound_type
    }

    /// Indicates whether this connection is an outbound connection
    pub fn is_outbound(&self) -> bool {
        self.outbound_type.is_some()
    }

    pub fn is_user_supplied(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::UserSupplied))
    }

    pub fn is_perigee(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::Perigee))
    }

    pub fn is_random_graph(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::RandomGraph))
    }

    pub fn connection_started(&self) -> Instant {
        self.connection_started
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

    pub fn perigee_timestamps(&self) -> Arc<HashMap<BlockHash, Instant>> {
        self.perigee_timestamps.clone()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct PeerKey {
    identity: PeerId,
    ip: IpAddress,
    /// port is ignored for equality and hashing, but useful for reconstructing the socket address from the key only.
    port: u16,
}

impl PeerKey {
    pub fn new(identity: PeerId, ip: IpAddress, port: u16) -> Self {
        Self { identity, ip, port }
    }

    pub fn sock_addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip.into(), self.port)
    }
}

impl Hash for PeerKey {
    // Custom hash implementation that ignores port
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.identity.hash(state);
        self.ip.hash(state);
    }
}

impl PartialEq for PeerKey {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity && self.ip == other.ip
    }
}

impl Eq for PeerKey {}

impl From<&Peer> for PeerKey {
    fn from(value: &Peer) -> Self {
        Self::new(value.identity, value.net_address.ip().into(), value.net_address.port())
    }
}

impl Display for PeerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}+{}", self.identity, self.ip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::net::IpAddr;
    use uuid::Uuid;

    #[test]
    fn test_peer_key_equality() {
        let peer1 = PeerKey::new(PeerId::new(Uuid::from_u128(1u128)), IpAddr::V4([192, 168, 1, 1].into()).into(), 8080);
        let peer2 = PeerKey::new(PeerId::new(Uuid::from_u128(1u128)), IpAddr::V4([192, 168, 1, 1].into()).into(), 9090);
        let peer3 = PeerKey::new(PeerId::new(Uuid::from_u128(2u128)), IpAddr::V4([192, 168, 1, 1].into()).into(), 8080);

        assert_eq!(peer1, peer2);
        assert_ne!(peer1, peer3);
    }

    #[test]
    fn test_peer_key_hashing() {
        let peer1 = PeerKey::new(PeerId::new(Uuid::from_u128(1u128)), IpAddr::V4([192, 168, 1, 1].into()).into(), 8080);

        let peer2 = PeerKey::new(PeerId::new(Uuid::from_u128(1u128)), IpAddr::V4([192, 168, 1, 1].into()).into(), 9090);

        let mut hasher1 = DefaultHasher::new();
        peer1.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        peer2.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        assert_eq!(hash1, hash2);
    }
}
