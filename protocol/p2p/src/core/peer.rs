use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_utils::networking::{IpAddress, PeerId, PrefixBucket};
use std::{fmt::Display, net::SocketAddr, sync::Arc, time::Instant};

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct PeerProperties {
    pub user_agent: String,
    // TODO: add services
    pub advertised_protocol_version: u32,
    pub protocol_version: u32,
    pub disable_relay_tx: bool,
    pub subnetwork_id: Option<SubnetworkId>,
    pub time_offset: i64,
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Peer {
    identity: PeerId,
    net_address: SocketAddr,
    is_outbound: bool,
    connection_started: Instant,
    properties: Arc<PeerProperties>,
    last_ping_duration: u64,
    last_block_transfer: Option<Instant>, // TODO: add this to rpc, currently sidelined due to ongoing RPC development
    last_tx_transfer: Option<Instant>,    // TODO: add this to rpc, currently sidelined due to ongoing RPC development
}

impl Peer {
    pub fn new(
        identity: PeerId,
        net_address: SocketAddr,
        is_outbound: bool,
        connection_started: Instant,
        properties: Arc<PeerProperties>,
        last_ping_duration: u64,
        last_block_transfer: Option<Instant>,
        last_tx_transfer: Option<Instant>,
    ) -> Self {
        Self {
            identity,
            net_address,
            is_outbound,
            connection_started,
            properties,
            last_ping_duration,
            last_block_transfer,
            last_tx_transfer,
        }
    }

    /// Internal identity of this peer
    pub fn identity(&self) -> PeerId {
        self.identity
    }

    /// The socket address of this peer
    pub fn net_address(&self) -> SocketAddr {
        self.net_address
    }

    pub fn prefix_bucket(&self) -> PrefixBucket {
        self.net_address.ip().into()
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

    pub fn last_block_transfer(&self) -> Option<Instant> {
        self.last_block_transfer
    }

    pub fn last_tx_transfer(&self) -> Option<Instant> {
        self.last_tx_transfer
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
