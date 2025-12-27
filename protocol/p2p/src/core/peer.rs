use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_utils::networking::{IpAddress, PeerId};
use std::{fmt::Display, net::SocketAddr, sync::Arc, time::Instant};

#[derive(Copy, Debug, Clone)]
pub enum PeerOutboundType {
    Perigee,
    RandomGraph,
    /// this is a user-specifed persistent connection, established either via command line `--connectpeer`, or the add_peer RPC (whereby is_permanent=true).
    /// These peers do not count towards the outbound limit, if they disconnect, the node will keep trying to reconnect to them indefinitely.
    Persistent,
    /// this is a user-specifed temporary connection, established either via command line `--addpeer`, or the add_peer RPC (whereby is_permanent=false).
    /// These peers do not count towards the outbound limit, if they disconnect, no effort will be made to reconnect.
    Temporary,
}

impl Display for PeerOutboundType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerOutboundType::Perigee => write!(f, "perigee"),
            PeerOutboundType::RandomGraph => write!(f, "random graph"),
            PeerOutboundType::Persistent => write!(f, "persistent"),
            PeerOutboundType::Temporary => write!(f, "temporary"),
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

#[derive(Debug)]
pub struct Peer {
    identity: PeerId,
    net_address: SocketAddr,
    outbound_type: Option<PeerOutboundType>,
    connection_started: Instant,
    properties: Arc<PeerProperties>,
    last_ping_duration: u64,
}

impl Peer {
    pub fn new(
        identity: PeerId,
        net_address: SocketAddr,
        outbound_type: Option<PeerOutboundType>,
        connection_started: Instant,
        properties: Arc<PeerProperties>,
        last_ping_duration: u64,
    ) -> Self {
        Self { identity, net_address, outbound_type, connection_started, properties, last_ping_duration }
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

    pub fn is_temporary(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::Temporary))
    }

    pub fn is_persistent(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::Persistent))
    }

    pub fn is_perigee(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::Perigee))
    }

    pub fn is_random_graph(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::RandomGraph))
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
