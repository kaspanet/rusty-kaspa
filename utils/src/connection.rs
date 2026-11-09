//! Advertised transport addresses and their authenticated updates.
use crate::networking::{IpAddress, NetAddress, PrefixBucket};
use alloc::vec::Vec;
use core::net::{IpAddr, SocketAddr};
use serde::{Deserialize, Serialize};

/// Ed25519 public-key bytes underlying the authenticated libp2p PeerId.
pub type Libp2pId = [u8; 32];

/// Address-book identity: an advertised TCP listener and optional signing identity.
pub type AddressKey = (NetAddress, Option<Libp2pId>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LibP2pAddress {
    pub address: SocketAddr,
    pub id: Libp2pId,
}

/// A node's advertised TCP listener, relay service, and libp2p route.
/// Signed records are forwarded unchanged; observed connection sockets are kept separately.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerAddress {
    /// Advertised IP shared by the Kaspa TCP listener and libp2p candidate.
    pub ip: IpAddress,
    /// Advertised Kaspa TCP port, which need not be reachable through NAT.
    pub tcp_port: u16,
    /// Identity, ports and owner signature for the libp2p transport, absent for legacy TCP records.
    pub libp2p: Option<Libp2pMetadata>,
    /// This node's current relay for coordinating hole punching, absent until a reservation is ready.
    pub relay_address: Option<LibP2pAddress>,
}

/// A node's authenticated libp2p identity, transport ports and signed address version.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Libp2pMetadata {
    /// Signing identity, authenticated again when connecting through libp2p.
    pub endpoint_id: Libp2pId,
    /// Advertised QUIC UDP candidate port; dialing learns current candidates through DCUtR.
    pub port: u16,
    /// Relay-service QUIC UDP port; zero withdraws the relay service while retaining the libp2p identity.
    pub relay_port: u16,
    /// Owner's persisted, increasing advertisement sequence number.
    pub nonce: u64,
    /// Ed25519 signature over the complete versioned, network-specific address payload.
    pub signature: Vec<u8>,
}

impl PeerAddress {
    pub fn endpoint_id(&self) -> Option<Libp2pId> {
        self.libp2p.as_ref().map(|metadata| metadata.endpoint_id)
    }

    pub fn libp2p_port(&self) -> Option<u16> {
        self.libp2p.as_ref().map(|metadata| metadata.port)
    }

    pub fn relay_port(&self) -> Option<u16> {
        self.libp2p.as_ref().map(|metadata| metadata.relay_port)
    }

    pub fn nonce(&self) -> Option<u64> {
        self.libp2p.as_ref().map(|metadata| metadata.nonce)
    }

    pub fn signature(&self) -> Option<&[u8]> {
        self.libp2p.as_ref().map(|metadata| metadata.signature.as_slice())
    }

    pub fn tcp_address(&self) -> NetAddress {
        NetAddress::new(self.ip.0.to_canonical().into(), self.tcp_port)
    }

    pub fn key(&self) -> AddressKey {
        (self.tcp_address(), self.endpoint_id())
    }

    pub fn prefix_bucket(&self) -> PrefixBucket {
        self.ip.prefix_bucket()
    }

    pub fn endpoint(&self) -> Option<EndpointAddress> {
        Some(EndpointAddress { id: self.endpoint_id()?, relay: self.relay_address })
    }

    /// Fixed v1 encoding; independent of protobuf field ordering and database serialization.
    pub fn signing_bytes(&self, network: &str) -> Vec<u8> {
        fn ip(bytes: &mut Vec<u8>, value: IpAddr) {
            let value = match value {
                IpAddr::V4(ip) => ip.to_ipv6_mapped(),
                IpAddr::V6(ip) => ip,
            };
            bytes.extend(value.octets());
        }

        let mut bytes = Vec::with_capacity(160);
        bytes.extend(b"kaspa/net-address/v1\0");
        bytes.extend((network.len() as u64).to_le_bytes());
        bytes.extend(network.as_bytes());
        ip(&mut bytes, self.ip.0);
        bytes.extend(self.tcp_port.to_le_bytes());
        bytes.push(u8::from(self.libp2p.is_some()));
        if let Some(metadata) = &self.libp2p {
            bytes.extend(metadata.endpoint_id);
            bytes.extend(metadata.port.to_le_bytes());
            bytes.extend(metadata.relay_port.to_le_bytes());
            bytes.extend(metadata.nonce.to_le_bytes());
        }
        bytes.push(u8::from(self.relay_address.is_some()));
        if let Some(address) = self.relay_address {
            ip(&mut bytes, address.address.ip());
            bytes.extend(address.address.port().to_le_bytes());
            bytes.extend(address.id);
        }
        bytes
    }

    /// Verifies the owner signature, or accepts a legacy TCP record with no extended fields.
    pub fn verify(&self, network: &str) -> bool {
        let Some(metadata) = &self.libp2p else {
            return self.relay_address.is_none();
        };
        if metadata.nonce == 0 {
            return false;
        }
        let Ok(key) = ed25519_dalek::VerifyingKey::from_bytes(&metadata.endpoint_id) else {
            return false;
        };
        let Ok(signature) = ed25519_dalek::Signature::from_slice(&metadata.signature) else {
            return false;
        };
        key.verify_strict(&self.signing_bytes(network), &signature).is_ok()
    }
}

impl From<NetAddress> for PeerAddress {
    fn from(address: NetAddress) -> Self {
        Self { ip: address.ip, tcp_port: address.port, libp2p: None, relay_address: None }
    }
}

/// Internal libp2p dialing route, derived from an advertised PeerAddress.
/// New connections start at the relay; DCUtR supplies candidates for the verified direct socket.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EndpointAddress {
    /// Destination identity authenticated by the libp2p transport.
    pub id: Libp2pId,
    /// Destination relay used to coordinate a direct connection.
    pub relay: Option<LibP2pAddress>,
}
