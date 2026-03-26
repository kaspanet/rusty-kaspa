use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::Arc;

use crate::servers::tcp_control::PeerDirection;
use arc_swap::ArcSwap;
use kaspa_core::{trace, warn};
use socket2::{Domain, Protocol, Socket, Type};

/// Create a connected UDP socket for sending to a specific peer.
///
/// The socket is connected to the target address, allowing use of `send()` instead of `send_to()`.
/// This can provide better performance by avoiding destination lookup per packet and enables
/// ICMP error reporting.
pub fn create_connected_socket(target: SocketAddr) -> io::Result<UdpSocket> {
    let domain = if target.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;

    // Set large send buffer to match the shared receive socket
    socket.set_send_buffer_size(32 * 1024 * 1024).ok();
    socket.set_nonblocking(false)?;

    // Connect to the peer address - this makes send() work without specifying destination
    socket.connect(&target.into())?;

    Ok(UdpSocket::from(socket))
}

pub type Allowlist = Arc<ArcSwap<HashMap<IpAddr, PeerDirection>>>;
pub type PeerInfoList = Arc<ArcSwap<Vec<PeerInfo>>>;

// ============================================================================
// PEER INFO — shared between TCP control and UDP fast-path
// ============================================================================

/// Shared peer metadata used by both control-plane (Hub) and data-plane (UDP).
///
/// Fields are public because this is a simple data carrier with no invariants
/// to enforce. Constructed via `PeerInfo::new()` and cheaply shared via `Arc`.
///
/// For outbound peers, a connected UDP socket is created for efficient sending
/// using `send()` instead of `send_to()`.
pub struct PeerInfo {
    pub address: SocketAddr,
    pub direction: PeerDirection,
    pub udp_target: SocketAddr,
    pub ready: bool,
    /// Connected UDP socket for this peer (outbound peers only).
    /// Using a connected socket allows `send()` instead of `send_to()`,
    /// which can be more efficient and provides ICMP error feedback.
    send_socket: Option<Arc<UdpSocket>>,
}

impl std::fmt::Debug for PeerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerInfo")
            .field("address", &self.address)
            .field("direction", &self.direction)
            .field("udp_target", &self.udp_target)
            .field("ready", &self.ready)
            .field("has_send_socket", &self.send_socket.is_some())
            .finish()
    }
}

impl Clone for PeerInfo {
    fn clone(&self) -> Self {
        Self {
            address: self.address,
            direction: self.direction,
            udp_target: self.udp_target,
            ready: self.ready,
            send_socket: self.send_socket.clone(),
        }
    }
}

impl PeerInfo {
    /// Create a new PeerInfo. For outbound peers, a connected UDP socket is created.
    pub fn new(address: SocketAddr, direction: PeerDirection, udp_target: SocketAddr) -> Self {
        let send_socket = if direction.is_outbound() {
            match create_connected_socket(udp_target) {
                Ok(socket) => {
                    trace!("PeerInfo: created connected socket for outbound peer {}", udp_target);
                    Some(Arc::new(socket))
                }
                Err(e) => {
                    warn!("PeerInfo: failed to create connected socket for {}: {}", udp_target, e);
                    None
                }
            }
        } else {
            None
        };
        Self { address, direction, udp_target, ready: false, send_socket }
    }

    /// Convenience: address accessor (also available via `.address` directly).
    #[inline]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Convenience: UDP target accessor.
    #[inline]
    pub fn udp_target(&self) -> SocketAddr {
        self.udp_target
    }

    /// Convenience: direction accessor.
    #[inline]
    pub fn direction(&self) -> PeerDirection {
        self.direction
    }

    /// Convenience: readiness accessor.
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Return a copy with the `ready` flag set.
    #[inline]
    pub fn with_ready(&self, ready: bool) -> Self {
        Self { ready, ..self.clone() }
    }

    /// Get the connected send socket for this peer (outbound peers only).
    #[inline]
    pub fn send_socket(&self) -> Option<&Arc<UdpSocket>> {
        self.send_socket.as_ref()
    }

    /// Whether this peer should receive outbound shards now.
    #[inline]
    pub fn is_outbound_ready(&self) -> bool {
        self.direction.is_outbound() && self.ready
    }

    /// Whether we accept inbound shards from this peer.
    #[inline]
    pub fn is_inbound_allowed(&self) -> bool {
        self.direction.is_inbound()
    }
}

// ============================================================================
// PEER DIRECTORY — shared read-mostly state for the UDP fast-path
// ============================================================================

/// A shared, read-mostly directory of connected peers.
///
/// **Writers** (rare): the Hub, on `PeerConnected` / `PeerDisconnected`.
/// **Readers** (hot): the `ShardForwarder` and `BroadcastWorker` on
/// every shard/block send — they call `snapshot()` to get a cheap
/// `Arc<Vec<Arc<PeerInfo>>>` and iterate it lock-free.
///
/// Implementation: an `ArcSwap` around `Arc<Vec<Arc<PeerInfo>>>`.
/// Writers clone-and-swap the Vec; readers load a shared Arc with one
/// atomic increment (no locks on the hot path).
///
pub struct PeerDirectory {
    /// Current snapshot of connected peers (cheaply cloned by UDP hot path).
    peer_infos: PeerInfoList,
    allowlist: Allowlist,
}

impl PeerDirectory {
    /// Create an empty directory.
    pub fn new(allow_list: HashMap<IpAddr, PeerDirection>) -> Self {
        Self { peer_infos: Arc::new(ArcSwap::from_pointee(Vec::new())), allowlist: Arc::new(ArcSwap::from_pointee(allow_list)) }
    }

    pub fn peer_info_list(&self) -> PeerInfoList {
        self.peer_infos.clone()
    }

    pub fn allowlist(&self) -> Allowlist {
        self.allowlist.clone()
    }

    /// Insert a peer into the directory.
    ///
    /// Called by the Hub on `PeerConnected`. Replaces any existing entry
    /// with the same `address`. If the existing peer is outbound and the new peer
    /// is also outbound with the same UDP target, the existing socket is reused.
    pub fn insert_peer(&self, mut peer: PeerInfo) {
        let old_vec = self.peer_infos.load_full();
        let old_allowlist = self.allowlist.load_full();
        let peer_addr = peer.address();

        // Check if we can reuse an existing socket from a peer with the same address
        if peer.direction.is_outbound() {
            if let Some(existing) = old_vec.iter().find(|p| p.address() == peer_addr) {
                // Reuse socket if existing peer is outbound with the same UDP target
                if existing.direction.is_outbound() && existing.udp_target == peer.udp_target {
                    if let Some(socket) = &existing.send_socket {
                        trace!("PeerDirectory: reusing existing socket for peer {}", peer_addr);
                        peer.send_socket = Some(socket.clone());
                    }
                }
            }
        }

        let mut new_vec: Vec<_> = old_vec.iter().filter(|p| p.address() != peer_addr).cloned().collect();
        new_vec.push(peer.clone());
        let len = new_vec.len();
        self.peer_infos.store(Arc::new(new_vec));
        // Also update the allowlist so the verifier worker accepts UDP packets from this peer.
        let mut new_allowlist = (*old_allowlist).clone();
        new_allowlist.insert(peer_addr.ip(), peer.direction());
        self.allowlist.store(Arc::new(new_allowlist));
        trace!("PeerDirectory: inserted peer, total={}", len);
    }

    /// Remove a peer from the directory.
    ///
    /// Called by the Hub on `PeerDisconnected`.
    /// Returns the removed `PeerInfo` if found.
    pub fn remove_peer(&self, address: &SocketAddr) -> Option<PeerInfo> {
        let old_vec = self.peer_infos.load_full();
        let position = old_vec.iter().position(|p| &p.address() == address)?;

        let mut new_vec = (*old_vec).clone();
        let removed = new_vec.remove(position);
        let len = new_vec.len();
        self.peer_infos.store(Arc::new(new_vec));
        trace!("PeerDirectory: removed peer {}, total={}", address, len);
        Some(removed)
    }
}

impl Default for PeerDirectory {
    fn default() -> Self {
        Self::new(HashMap::new())
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servers::tcp_control::PeerDirection;
    use std::sync::Arc;

    fn meta(port: u16, direction: PeerDirection, ready: bool) -> PeerInfo {
        PeerInfo::new(format!("127.0.0.1:{}", port).parse().unwrap(), direction, format!("127.0.0.1:{}", port + 1000).parse().unwrap())
            .with_ready(ready)
    }

    #[test]
    fn insert_and_snapshot() {
        let dir = PeerDirectory::new(HashMap::new());
        assert!(dir.peer_info_list().load_full().is_empty());

        dir.insert_peer(meta(1000, PeerDirection::Outbound, true));
        dir.insert_peer(meta(2000, PeerDirection::Inbound, false));

        let snap = dir.peer_info_list().load_full();
        assert_eq!(snap.len(), 2);
        assert_eq!(dir.peer_info_list().load_full().len(), 2);
    }

    #[test]
    fn remove_peer() {
        let dir = PeerDirectory::new(HashMap::new());
        let m = meta(1000, PeerDirection::Outbound, true);
        dir.insert_peer(m.clone());
        assert_eq!(dir.peer_info_list().load_full().len(), 1);

        let removed = dir.remove_peer(&"127.0.0.1:1000".parse().unwrap());
        assert!(removed.is_some());
        assert!(dir.peer_info_list().load_full().is_empty());

        // Removing again is a no-op.
        let removed = dir.remove_peer(&"127.0.0.1:1000".parse().unwrap());
        assert!(removed.is_none());
    }

    #[test]
    fn insert_replaces_existing() {
        let dir = PeerDirectory::new(HashMap::new());
        dir.insert_peer(meta(1000, PeerDirection::Outbound, true));
        dir.insert_peer(meta(1000, PeerDirection::Both, false));

        let snap = dir.peer_info_list().load_full();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].direction(), PeerDirection::Both);
    }

    #[test]
    fn snapshot_is_independent_of_mutations() {
        let dir = PeerDirectory::new(HashMap::new());
        dir.insert_peer(meta(1000, PeerDirection::Outbound, true));

        let snap = dir.peer_info_list().load_full();
        assert_eq!(snap.len(), 1);

        // Mutate the directory — the existing snapshot must not change.
        dir.insert_peer(meta(2000, PeerDirection::Inbound, false));
        assert_eq!(snap.len(), 1); // still 1
        assert_eq!(dir.peer_info_list().load_full().len(), 2); // directory has 2
    }

    #[test]
    fn is_outbound_ready_checks_both_fields() {
        let m = meta(1000, PeerDirection::Outbound, true);
        assert!(m.is_outbound_ready());

        let m = meta(1000, PeerDirection::Outbound, false);
        assert!(!m.is_outbound_ready());

        let m = meta(1000, PeerDirection::Inbound, true);
        assert!(!m.is_outbound_ready());

        let m = meta(1000, PeerDirection::Both, true);
        assert!(m.is_outbound_ready());
    }
}
