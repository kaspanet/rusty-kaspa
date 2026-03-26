use std::net::SocketAddr;
use std::sync::Arc;

use kaspa_utils::triggers::Listener;
use kaspa_core::{debug, info, warn};
use rand::{RngCore, rngs::OsRng};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use crate::error::{RelayError, RelayResult};
use crate::fast_trusted_relay::{DEFAULT_TCP_PORT, DEFAULT_UDP_PORT};
use crate::servers::auth::{self, TokenAuthenticator};
use crate::servers::peer_directory::{Allowlist, PeerDirectory};
use crate::servers::tcp_control::HubEvent;
use crate::servers::tcp_control::{Peer, PeerDirection};

/// Handshake wire format (kept simple):
///
/// ```text
/// Client → Server:  [hmac(secret, nonce‖direction‖udp_port): 32 bytes] [nonce:32 bytes] [direction: 1 byte] [udp_port: 2 bytes LE]
/// Server → Client:  [0x01 = OK | 0x00 = REJECT]
/// ```
///
/// The token is transmitted first so the server can immediately start
/// validating the authenticity of the message.  `direction` is encoded from
/// the client’s perspective: 0x01 = Inbound (client receives shards),
/// 0x02 = Outbound (client sends shards), 0x03 = Both.
pub struct TcpServer {
    listen_address: SocketAddr,
    authenticator: Arc<TokenAuthenticator>,
    hub_event_sender: mpsc::UnboundedSender<HubEvent>,
    shutdown_listen: Listener,
    directory: Arc<PeerDirectory>,
}

impl TcpServer {
    pub fn new(
        listen_address: SocketAddr,
        authenticator: Arc<TokenAuthenticator>,
        hub_event_sender: mpsc::UnboundedSender<HubEvent>,
        shutdown_listen: Listener,
        directory: Arc<PeerDirectory>,
    ) -> Self {
        Self { listen_address, authenticator, hub_event_sender, shutdown_listen, directory }
    }

    /// Run the accept loop. Spawns a task per incoming TCP connection to
    /// perform the handshake, then registers the peer with the Hub.
    pub async fn run(&mut self) {
        let tcp_listener = TcpListener::bind(format!("0.0.0.0:{}", self.listen_address.port())).await.unwrap();

        let local_addr = tcp_listener.local_addr().map(|addr| addr.to_string()).unwrap_or_else(|_| "unknown".to_string());
        info!("TCP server listening on {}", local_addr);

        // Perform immediate connection attempts to all outgoing peers on startup
        self.attempt_reconnections().await;

        let shutdown_listener = self.shutdown_listen.clone();
        loop {
            select! {
                    // shutdown signal
                     _ = shutdown_listener.clone() => {
                        info!("TCP server shutting down");
                        break;
                    },
                    // tcp accept
                    tcp_accept = tcp_listener.accept() => {
                        match tcp_accept {
                            Ok((stream, addr)) => {
                                debug!("TCP connection from {}", addr);
                                let hub_tx = self.hub_event_sender.clone();
                                let directory = self.directory.clone();
                                let authenticator = self.authenticator.clone();
                                tokio::spawn(async move {
                                    match handshake_accept(stream, addr, authenticator, hub_tx.clone(), directory.allowlist().clone()).await {
                                        Ok(peer) => {
                                            info!("Peer {} authenticated ({})", addr, peer.direction());
                                            if hub_tx.send(HubEvent::PeerConnected(peer)).is_err() {
                                                warn!("Hub channel closed, dropping peer {}", addr);
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Handshake failed from {}: {}", addr, e);
                                        }
                                    }
                                });
                            }
                            Err(e) => {
                                warn!("TCP accept error: {}", e);
                            }
                        }
                    },
                // reconnection attempts
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    self.attempt_reconnections().await;
                },
            }
        }
    }

    /// Attempt to connect to all outgoing peers that are not already connected.
    async fn attempt_reconnections(&self) {
        info!("Fast Trusted Relay: checking for peers to connect...");
        // this is a dynamically created list of currently connected peers,
        // we may use this to filter reconnection attempts.
        let peer_info_list = self.directory.peer_info_list().load_full();
        // this is a static list of allowed addresses, and directions, from which we will attempt reconnections.
        let allow_list = self.directory.allowlist().load_full();

        let mut attempted = 0;
        for (a, direction) in allow_list.iter() {
            if direction == &PeerDirection::Inbound {
                continue; // we only attempt reconnections to peers we are supposed to send to.
            }
            if !peer_info_list.iter().any(|p| &p.address().ip() == a) {
                attempted += 1;
                info!("Fast Trusted Relay: attempting connection to peer {} (direction: {:?})", a, direction);
                match tcp_connect(
                    SocketAddr::from((*a, DEFAULT_TCP_PORT)),
                    self.authenticator.clone(),
                    *direction,
                    0,
                    self.hub_event_sender.clone(),
                    self.directory.allowlist(),
                )
                .await
                {
                    Ok(_) => info!("Fast Trusted Relay: connection to peer {} succeeded", a),
                    Err(e) => info!("Fast Trusted Relay: connection to peer {} failed: {}", a, e),
                }
            }
        }

        if attempted == 0 {
            info!(
                "Fast Trusted Relay: no outgoing peers to connect (allowlist: {} entries, connected: {})",
                allow_list.len(),
                peer_info_list.len()
            );
        }
    }
}

/// Size of the client → server handshake message.
const HANDSHAKE_MSG_SIZE: usize = 32 + 32 + 1 + 2; // nonce + hmac + direction + udp_port

/// Maximum time to wait for a handshake to complete before giving up.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

async fn handshake_accept(
    mut stream: TcpStream,
    addr: SocketAddr,
    authenticator: Arc<TokenAuthenticator>,
    hub_event_tx: mpsc::UnboundedSender<HubEvent>,
    allow_list: Allowlist,
) -> RelayResult<Peer> {
    if !allow_list.load_full().contains_key(&addr.ip()) {
        return Err(RelayError::PeerConnection(format!("peer {} not in allowlist", addr)));
    }

    let mut buf = [0u8; HANDSHAKE_MSG_SIZE];
    timeout(HANDSHAKE_TIMEOUT, stream.read_exact(&mut buf))
        .await
        .map_err(|_| RelayError::PeerConnection(format!("handshake read timed out for {}", addr)))?
        .map_err(|e| RelayError::PeerConnection(format!("handshake read from {}: {}", addr, e)))?;

    // message layout: [token][nonce][direction][udp_port]
    // our goal: extract nonce and token in the same order the client
    // transmitted them, then compute HMAC over `direction||udp_port` exactly
    // as the client did.
    let client_hmac = &buf[0..32];
    let nonce = &buf[32..64];
    let direction_byte = buf[64];
    //let udp_port = u16::from_le_bytes([buf[65], buf[66]]);

    // data used in the HMAC is everything following the nonce (direction + udp_port)
    let data_for_hmac = &buf[64..];

    // Validate HMAC(secret, nonce || SHA256(direction||udp_port)).
    let nonce_array: [u8; 32] = nonce.try_into().map_err(|_| RelayError::AuthenticationFailed("invalid nonce size".into()))?;
    let their_token = auth::AuthToken::from_bytes(client_hmac.to_vec());
    let is_authentic = authenticator.validate_token(&nonce_array, &data_for_hmac, &their_token);
    if !is_authentic {
        info!("msg auth failed expected HMAC");
        let _ = stream.write_all(&[0x00]).await;
        return Err(RelayError::AuthenticationFailed(format!("HMAC mismatch from {}", addr)));
    }

    // Respond OK.
    stream.write_all(&[0x01]).await.map_err(|e| RelayError::Io(e))?;

    // Map client-perspective direction to our perspective (invert).
    let direction = match direction_byte {
        0x01 => PeerDirection::Outbound, // client receives → we send
        0x02 => PeerDirection::Inbound,  // client sends → we receive
        0x03 => PeerDirection::Both,
        _ => return Err(RelayError::PeerConnection(format!("invalid direction byte 0x{:02x}", direction_byte))),
    };

    let udp_target = SocketAddr::new(addr.ip(), DEFAULT_UDP_PORT);
    Ok(Peer::new(addr, direction, stream, udp_target, hub_event_tx))
}

/// Connect to a remote peer, perform the handshake, and register with Hub.
///
/// `our_direction` is from *our* perspective (Inbound = we receive shards).
/// `local_udp_port` is the port of our shared UDP socket so the remote knows
/// where to send shards.
pub async fn tcp_connect(
    remote_addr: SocketAddr,
    authenticator: Arc<TokenAuthenticator>,
    our_direction: PeerDirection,
    local_udp_port: u16,
    hub_event_sender: mpsc::UnboundedSender<HubEvent>,
    allow_list: Allowlist,
) -> RelayResult<SocketAddr> {
    if !allow_list.load_full().contains_key(&remote_addr.ip()) {
        return Err(RelayError::PeerConnection(format!("peer {} not in allowlist", remote_addr)));
    }

    let mut stream = timeout(HANDSHAKE_TIMEOUT, TcpStream::connect(remote_addr))
        .await
        .map_err(|_| RelayError::PeerConnection(format!("connect timed out to {}", remote_addr)))?
        .map_err(|e| RelayError::PeerConnection(format!("connect to {}: {}", remote_addr, e)))?;

    // Generate cryptographically secure nonce + HMAC.
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);

    // Direction byte: from *our* perspective.
    let direction_byte = match our_direction {
        PeerDirection::Inbound => 0x01,
        PeerDirection::Outbound => 0x02,
        PeerDirection::Both => 0x03,
    };

    // original layout: token first, then nonce
    let mut msg = [0u8; HANDSHAKE_MSG_SIZE];
    msg[32..64].copy_from_slice(nonce.as_ref());
    msg[64] = direction_byte;
    //msg[65..67].copy_from_slice(&local_udp_port.to_le_bytes());
    // token computed over nonce and direction+port
    let token = authenticator.generate_token(&nonce, &msg[64..]);
    msg[0..32].copy_from_slice(&token.as_bytes());

    // perform the write with an explicit result type so the compiler
    // can infer the intermediate `Result` produced by `timeout`.
    let write_result: std::io::Result<()> = timeout(HANDSHAKE_TIMEOUT, stream.write_all(&msg))
        .await
        .map_err(|_| RelayError::PeerConnection(format!("handshake write timed out to {}", remote_addr)))?;
    write_result.map_err(|e| RelayError::PeerConnection(format!("handshake write to {}: {}", remote_addr, e)))?;

    // Read response.
    let mut resp = [0u8; 1];
    timeout(HANDSHAKE_TIMEOUT, stream.read_exact(&mut resp))
        .await
        .map_err(|_| RelayError::PeerConnection(format!("handshake response timed out from {}", remote_addr)))?
        .map_err(|e| RelayError::PeerConnection(format!("handshake response read from {}: {}", remote_addr, e)))?;

    if resp[0] != 0x01 {
        return Err(RelayError::AuthenticationFailed(format!("remote {} rejected handshake", remote_addr)));
    }

    let peer_addr = stream.peer_addr()?;
    let udp_target = SocketAddr::new(peer_addr.ip(), DEFAULT_UDP_PORT);
    let peer = Peer::new(peer_addr, our_direction, stream, udp_target, hub_event_sender.clone());
    hub_event_sender
        .send(HubEvent::PeerConnected(peer))
        .map_err(|_| RelayError::ChannelSend("hub channel closed during connect".into()))?;

    Ok(peer_addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_handshake_roundtrip() {
        let secret = b"test-secret".to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        let (hub_tx, mut hub_rx) = mpsc::unbounded_channel::<HubEvent>();

        // Server side: accept + handshake.
        let server_secret = secret.clone();
        let server_hub_tx = hub_tx.clone();
        // Allow all loopback addresses (the server will accept any client connecting from 127.0.0.1)
        let mut server_allowlist = std::collections::HashMap::new();
        // Add a broad range of loopback addresses
        for __port in 0u16..65535 {
            server_allowlist
                .insert(std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), DEFAULT_TCP_PORT).ip(), PeerDirection::Both);
        }
        let server_allow_list = Arc::new(arc_swap::ArcSwap::from_pointee(server_allowlist));
        let server_handle = tokio::spawn(async move {
            let (stream, addr) = listener.accept().await.unwrap();
            handshake_accept(stream, addr, Arc::new(TokenAuthenticator::new(server_secret)), server_hub_tx, server_allow_list).await
        });

        // Client side: connect + handshake.
        let authenticator = Arc::new(TokenAuthenticator::new(secret.clone()));
        let allow_list =
            Arc::new(arc_swap::ArcSwap::from_pointee(vec![(server_addr.ip(), PeerDirection::Both)].into_iter().collect()));
        let client_hub_tx = hub_tx.clone();
        let client_handle = tokio::spawn(async move {
            tcp_connect(server_addr, authenticator, PeerDirection::Both, 9999, client_hub_tx, allow_list).await
        });

        // Server result check (if peer was created)
        let server_result = server_handle.await.unwrap();
        assert!(server_result.is_ok(), "Server handshake failed: {:?}", server_result);
        let peer_from_server = server_result.unwrap();
        assert_eq!(peer_from_server.direction(), PeerDirection::Both);

        let client_result = client_handle.await.unwrap();
        assert!(client_result.is_ok(), "Client handshake failed: {:?}", client_result);

        // Hub should have received a PeerConnected from the client path.
        let event = hub_rx.recv().await.unwrap();
        assert!(matches!(event, HubEvent::PeerConnected(_)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_handshake_rejects_bad_secret() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        let (hub_tx, _hub_rx) = mpsc::unbounded_channel::<HubEvent>();

        let server_hub_tx = hub_tx.clone();
        // Allow all loopback addresses
        let mut server_allowlist = std::collections::HashMap::new();
        for _port in 0u16..65535 {
            server_allowlist
                .insert(std::net::SocketAddr::new("127.0.0.1".parse().unwrap(), DEFAULT_TCP_PORT).ip(), PeerDirection::Inbound);
        }
        let server_allow_list = Arc::new(arc_swap::ArcSwap::from_pointee(server_allowlist));
        let server_handle = tokio::spawn(async move {
            let (stream, addr) = listener.accept().await.unwrap();
            handshake_accept(
                stream,
                addr,
                Arc::new(TokenAuthenticator::new(b"correct-secret".to_vec())),
                server_hub_tx,
                server_allow_list,
            )
            .await
        });

        let wrong_authenticator = Arc::new(TokenAuthenticator::new(b"wrong-secret".to_vec()));
        let allow_list =
            Arc::new(arc_swap::ArcSwap::from_pointee(vec![(server_addr.ip(), PeerDirection::Inbound)].into_iter().collect()));
        let client_hub_tx = hub_tx.clone();
        let client_handle = tokio::spawn(async move {
            tcp_connect(server_addr, wrong_authenticator, PeerDirection::Inbound, 9999, client_hub_tx, allow_list).await
        });

        let server_result = server_handle.await.unwrap();
        assert!(server_result.is_err());

        let client_result = client_handle.await.unwrap();
        assert!(client_result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_handshake_detects_tampered_payload() {
        let secret = b"test-secret".to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        let (hub_tx, _hub_rx) = mpsc::unbounded_channel::<HubEvent>();

        // server will independently compute the expected token
        let server_allow_list =
            Arc::new(arc_swap::ArcSwap::from_pointee(vec![(server_addr.ip(), PeerDirection::Both)].into_iter().collect()));
        let server_secret = secret.clone();
        let server_handle = tokio::spawn(async move {
            let (stream, addr) = listener.accept().await.unwrap();
            handshake_accept(stream, addr, Arc::new(TokenAuthenticator::new(server_secret)), hub_tx, server_allow_list).await
        });

        // craft a valid handshake in the original token-first layout, then
        // tamper with the direction byte after the token has been calculated.
        let mut client = TcpStream::connect(server_addr).await.unwrap();
        let mut nonce = [0u8; 32];
        OsRng.fill_bytes(&mut nonce);
        let mut msg = [0u8; HANDSHAKE_MSG_SIZE];
        msg[32..64].copy_from_slice(&nonce);
        msg[64] = 0x01; // inbound
        let token = TokenAuthenticator::new(secret.clone()).generate_token(&nonce, &msg[64..]);
        msg[0..32].copy_from_slice(&token.as_bytes());
        // tamper direction
        msg[64] = 0x02;
        client.write_all(&msg).await.unwrap();
        let mut resp = [0u8; 1];
        let _ = timeout(HANDSHAKE_TIMEOUT, client.read_exact(&mut resp)).await;
        assert_eq!(resp[0], 0x00);

        let server_result = server_handle.await.unwrap();
        assert!(server_result.is_err());
    }
}
