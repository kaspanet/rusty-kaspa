use std::fmt;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;

use kaspa_core::{debug, trace, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::servers::tcp_control::HubEvent;
use crate::servers::peer_directory::PeerInfo;


// ============================================================================
// PEER DIRECTION
// ============================================================================

/// Whether this peer sends us shards, receives shards from us, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PeerDirection {
    /// We receive shards from this peer.
    Inbound = 1u8,
    /// We send shards to this peer.
    Outbound = 2u8,
    /// Bidirectional — we both send and receive.
    Both = 3u8,
}

impl PeerDirection {
    pub fn is_inbound(self) -> bool {
        matches!(self, PeerDirection::Inbound | PeerDirection::Both)
    }

    pub fn is_outbound(self) -> bool {
        matches!(self, PeerDirection::Outbound | PeerDirection::Both)
    }
}

impl fmt::Display for PeerDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerDirection::Inbound => write!(f, "inbound"),
            PeerDirection::Outbound => write!(f, "outbound"),
            PeerDirection::Both => write!(f, "bidirectional"),
        }
    }
}

// ============================================================================
// CONTROL MESSAGES — sent over the TCP control channel
// ============================================================================

/// Control commands exchanged over the TCP stream.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ControlMsg {
    /// Begin relaying blocks.
    Start = 1u8,
    /// Stop relaying blocks.
    Stop = 2u8,
    /// Graceful disconnect.
    Shutdown = 3u8,
    /// Keepalive ping.
    Ping = 4u8,
    /// Keepalive pong.
    Pong = 5u8,
}

impl From<&ControlMsg> for u8 {
    fn from(msg: &ControlMsg) -> Self {
        *msg as u8
    }
}

impl TryFrom<u8> for ControlMsg {
    type Error = u8;

    /// Convert a wire tag byte into a `ControlMsg`.
    ///
    /// Returns `Err(tag)` for unrecognised values instead of panicking.
    fn try_from(tag: u8) -> Result<Self, u8> {
        match tag {
            1 => Ok(ControlMsg::Start),
            2 => Ok(ControlMsg::Stop),
            3 => Ok(ControlMsg::Shutdown),
            4 => Ok(ControlMsg::Ping),
            5 => Ok(ControlMsg::Pong),
            other => Err(other),
        }
    }
}

impl ControlMsg {
    /// Encode a `ControlMsg` into a 3-byte length-prefixed frame for TCP transmission.
    ///
    /// Format: `[length: u16 LE][tag: u8]` — no heap allocation.
    pub fn encode(&self) -> [u8; 3] {
        let len_bytes = 1u16.to_le_bytes();
        [len_bytes[0], len_bytes[1], *self as u8]
    }

    /// Decode a `ControlMsg` from a tag byte.
    ///
    /// Alias for `ControlMsg::try_from(tag).ok()`.
    pub fn decode(tag: u8) -> Option<Self> {
        Self::try_from(tag).ok()
    }
}

// ============================================================================
// PEER
// ============================================================================

/// A connected trusted-relay peer.
///
/// Each peer has:
/// - A **TCP stream** for control commands (START / STOP / SHUTDOWN / PING).
/// - A **UDP target address** where outbound shards are sent. The actual
///   `UdpSocket` is owned by the Hub (single shared socket for all peers).
pub struct Peer {
    /// Shared metadata used by both the Hub and UDP fast-path.
    peer_info: Arc<PeerInfo>,
    /// TCP stream for the control channel.
    tcp_stream: TcpStream,
    /// Sender half for injecting local control commands (e.g. from Hub).
    control_tx: mpsc::Sender<ControlMsg>,
    /// Receiver half — consumed by `run_control_loop`.
    control_rx: Option<mpsc::Receiver<ControlMsg>>,
    /// Sender back to the Hub for readiness and disconnect events.
    hub_event_tx: mpsc::UnboundedSender<HubEvent>,
}

impl Peer {
    /// Create a new peer.
    ///
    /// `tcp_stream` is the already-connected (and authenticated) TCP stream.
    /// `udp_target` is the address to which outbound shards are sent via the
    /// Hub's shared UDP socket.
    pub fn new(
        address: SocketAddr,
        direction: PeerDirection,
        tcp_stream: TcpStream,
        udp_target: SocketAddr,
        hub_event_tx: mpsc::UnboundedSender<HubEvent>,
    ) -> Self {
        let (control_tx, control_rx) = mpsc::channel(64);
        let peer_info = Arc::new(PeerInfo::new(address, direction, udp_target));
        Self { peer_info, tcp_stream, control_tx, control_rx: Some(control_rx), hub_event_tx }
    }

    // -- Accessors -----------------------------------------------------------

    pub fn address(&self) -> SocketAddr {
        self.peer_info.address()
    }

    pub fn direction(&self) -> PeerDirection {
        self.peer_info.direction()
    }

    pub fn is_inbound(&self) -> bool {
        self.direction().is_inbound()
    }

    pub fn is_outbound(&self) -> bool {
        self.direction().is_outbound()
    }

    pub fn udp_target(&self) -> SocketAddr {
        self.peer_info.udp_target()
    }

    /// Clone the local control-command sender (used by Hub to inject commands).
    pub fn control_tx(&self) -> mpsc::Sender<ControlMsg> {
        self.control_tx.clone()
    }

    /// peer metadata used by Hub / PeerDirectory / transport.
    pub fn peer_info(&self) -> &PeerInfo {
        &self.peer_info
    }

    // -- Control loop --------------------------------------------------------

    /// Run the TCP control loop for this peer.
    ///
    /// Multiplexes between:
    /// 1. Reading length-prefixed frames from the TCP stream (remote commands).
    /// 2. Receiving local `ControlMsg` values (injected by Hub / Adaptor).
    ///
    /// Returns when the peer should be disconnected (Shutdown received,
    /// stream closed, or channel closed).
    pub async fn run_control_loop(&mut self) -> PeerCloseReason {
        // take ownership of the local control receiver; return AlreadyRan if called twice
        let mut control_rx = match self.control_rx.take() {
            Some(rx) => rx,
            None => return PeerCloseReason::AlreadyRan,
        };

        loop {
            tokio::select! {
                // Local control channel (injected commands).
                maybe_cmd = control_rx.recv() => {
                    match maybe_cmd {
                        Some(ControlMsg::Shutdown) => {
                            debug!("Peer {} local Shutdown", self.address());
                            return PeerCloseReason::LocalShutdown;
                        },
                        Some(ControlMsg::Ping) => {
                            trace!("Peer {} local ping, sending pong", self.address());
                            let pong = ControlMsg::Pong.encode();
                            if self.tcp_stream.write_all(&pong).await.is_err() {
                                return PeerCloseReason::WriteError;
                            }
                        },
                        Some(ControlMsg::Start) => {
                            trace!("Peer {} local START — sending over TCP", self.address());
                            let frame = ControlMsg::Start.encode();
                            if self.tcp_stream.write_all(&frame).await.is_err() {
                                return PeerCloseReason::WriteError;
                            }
                        },
                        Some(ControlMsg::Stop) => {
                            trace!("Peer {} local STOP — sending over TCP", self.address());
                            let frame = ControlMsg::Stop.encode();
                            if self.tcp_stream.write_all(&frame).await.is_err() {
                                return PeerCloseReason::WriteError;
                            }
                        },
                        Some(_) => {},
                        None => {
                            debug!("Peer {} control channel closed", self.address());
                            return PeerCloseReason::ChannelClosed;
                        }
                    }
                },

                // Remote TCP control frames (length-prefixed).
                res = async {
                    let len = self.tcp_stream.read_u16_le().await?;
                    let mut buf = vec![0u8; len as usize];
                    self.tcp_stream.read_exact(&mut buf).await?;
                    Ok::<Vec<u8>, std::io::Error>(buf)
                } => {
                    match res {
                        Ok(buf) => {
                            if buf.is_empty() {
                                debug!("Peer {} TCP stream closed", self.address());
                                return PeerCloseReason::StreamClosed;
                            }
                            let tag = buf[0];
                            match ControlMsg::decode(tag) {
                                Some(ControlMsg::Shutdown) => {
                                    debug!("Peer {} sent Shutdown", self.address());
                                    return PeerCloseReason::RemoteShutdown;
                                },
                                Some(ControlMsg::Ping) => {
                                    trace!("Peer {} ping, sending pong", self.address());
                                    let pong = ControlMsg::Pong.encode();
                                    if self.tcp_stream.write_all(&pong).await.is_err() {
                                        return PeerCloseReason::WriteError;
                                    }
                                },
                                Some(ControlMsg::Pong) => {
                                    trace!("Peer {} pong received", self.address());
                                },
                                Some(ControlMsg::Start) => {
                                    trace!("Peer {} remote START — peer is ready", self.address());
                                    let _ = self.hub_event_tx.send(HubEvent::PeerReady(self.address(), true));
                                },
                                Some(ControlMsg::Stop) => {
                                    trace!("Peer {} remote STOP — peer is not ready", self.address());
                                    let _ = self.hub_event_tx.send(HubEvent::PeerReady(self.address(), false));
                                },
                                None => {
                                    warn!("Peer {} sent unknown tag 0x{:02x}", self.address(), tag);
                                },
                            }
                        }
                        Err(e) => {
                            if e.kind() == ErrorKind::UnexpectedEof {
                                debug!("Peer {} TCP stream closed", self.address());
                                return PeerCloseReason::StreamClosed;
                            } else {
                                warn!("Peer {} TCP read error: {}", self.address(), e);
                                return PeerCloseReason::ReadError;
                            }
                        }
                    }
                },
            }
        }
    }
}

impl fmt::Debug for Peer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Peer")
            .field("address", &self.address())
            .field("direction", &self.direction())
            .field("udp_target", &self.udp_target())
            .finish()
    }
}

// ============================================================================
// PEER CLOSE REASON
// ============================================================================

/// Why the control loop exited — returned to the Hub for logging / cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerCloseReason {
    RemoteShutdown,
    LocalShutdown,
    StreamClosed,
    ReadError,
    WriteError,
    ChannelClosed,
    AlreadyRan,
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// Helper: create a connected TCP pair and wrap one side in a Peer.
    async fn make_peer_and_remote() -> (Peer, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let connect_fut = TcpStream::connect(addr);
        let accept_fut = listener.accept();
        let (remote_result, accept_result) = tokio::join!(connect_fut, accept_fut);
        let remote_stream = remote_result.unwrap();
        let (server_stream, _) = accept_result.unwrap();

        let (hub_tx, _hub_rx) = mpsc::unbounded_channel();
        let peer = Peer::new(addr, PeerDirection::Both, server_stream, "127.0.0.1:9999".parse().unwrap(), hub_tx);
        (peer, remote_stream)
        }

    #[tokio::test]
    async fn test_control_msg_roundtrip() {
        let msgs = [ControlMsg::Start, ControlMsg::Stop, ControlMsg::Shutdown, ControlMsg::Ping, ControlMsg::Pong];
        for msg in &msgs {
            let encoded = msg.encode();
            // Skip the 2-byte length prefix to get the tag.
            let tag = encoded[2];
            let decoded = ControlMsg::decode(tag).unwrap();
            assert_eq!(&decoded, msg);
        }
    }

    #[tokio::test]
    async fn test_remote_shutdown_closes_loop() {
        let (mut peer, mut remote) = make_peer_and_remote().await;

        // Remote sends a Shutdown frame.
        let frame = ControlMsg::Shutdown.encode();
        remote.write_all(&frame).await.unwrap();

        let reason = peer.run_control_loop().await;
        assert_eq!(reason, PeerCloseReason::RemoteShutdown);
    }

    #[tokio::test]
    async fn test_local_shutdown_closes_loop() {
        let (mut peer, mut _remote) = make_peer_and_remote().await;
        let tx = peer.control_tx();

        tokio::spawn(async move {
            tx.send(ControlMsg::Shutdown).await.unwrap();
        });

        let reason = peer.run_control_loop().await;
        assert_eq!(reason, PeerCloseReason::LocalShutdown);
    }

    #[tokio::test]
    async fn test_stream_close_detected() {
        let (mut peer, remote) = make_peer_and_remote().await;
        drop(remote); // close the TCP stream

        let reason = peer.run_control_loop().await;
        assert_eq!(reason, PeerCloseReason::StreamClosed);
    }

    #[tokio::test]
    async fn test_ping_receives_pong() {
        let (mut peer, mut remote) = make_peer_and_remote().await;

        // Send Ping from remote.
        let ping_frame = ControlMsg::Ping.encode();
        remote.write_all(&ping_frame).await.unwrap();

        // Then shutdown so the loop exits.
        let shutdown_frame = ControlMsg::Shutdown.encode();
        remote.write_all(&shutdown_frame).await.unwrap();

        let reason = peer.run_control_loop().await;
        assert_eq!(reason, PeerCloseReason::RemoteShutdown);

        // Read the pong that was sent back.
        let mut buf = [0u8; 3];
        remote.read_exact(&mut buf).await.unwrap();
        let tag = buf[2];
        assert_eq!(ControlMsg::decode(tag), Some(ControlMsg::Pong));
    }

    #[test]
    fn test_peer_direction() {
        assert!(PeerDirection::Inbound.is_inbound());
        assert!(!PeerDirection::Inbound.is_outbound());
        assert!(!PeerDirection::Outbound.is_inbound());
        assert!(PeerDirection::Outbound.is_outbound());
        assert!(PeerDirection::Both.is_inbound());
        assert!(PeerDirection::Both.is_outbound());
    }
}
