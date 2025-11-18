use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_utils::networking::{AddressKind, NetAddress, PeerId};
use std::{fmt::Display, sync::Arc, time::Instant};

#[derive(Debug, Clone, Default)]
pub struct PeerProperties {
    pub user_agent: String,
    pub services: u64,
    pub advertised_protocol_version: u32,
    pub protocol_version: u32,
    pub disable_relay_tx: bool,
    pub subnetwork_id: Option<SubnetworkId>,
    pub time_offset: i64,
    pub supports_addrv2: bool,
}

#[derive(Debug)]
pub struct Peer {
    identity: PeerId,
    net_address: NetAddress,
    is_outbound: bool,
    connection_started: Instant,
    properties: Arc<PeerProperties>,
    last_ping_duration: u64,
}

impl Peer {
    pub fn new(
        identity: PeerId,
        net_address: NetAddress,
        is_outbound: bool,
        connection_started: Instant,
        properties: Arc<PeerProperties>,
        last_ping_duration: u64,
    ) -> Self {
        Self { identity, net_address, is_outbound, connection_started, properties, last_ping_duration }
    }

    /// Internal identity of this peer
    pub fn identity(&self) -> PeerId {
        self.identity
    }

    /// The socket address of this peer
    pub fn net_address(&self) -> NetAddress {
        self.net_address
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
pub struct PeerKey {
    identity: PeerId,
    address: AddressKind,
}

impl PeerKey {
    pub fn new(identity: PeerId, address: AddressKind) -> Self {
        Self { identity, address }
    }
}

impl From<&Peer> for PeerKey {
    fn from(value: &Peer) -> Self {
        Self::new(value.identity, value.net_address.kind())
    }
}

impl Display for PeerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}+{}", self.identity, self.address)
    }
}
