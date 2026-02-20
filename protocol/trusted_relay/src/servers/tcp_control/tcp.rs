use std::net::SocketAddr;

use log::{debug, info, warn};
use rand::{RngCore, rngs::OsRng};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use crate::auth::TokenAuthenticator;
use crate::error::{RelayError, RelayResult};
use crate::servers::tcp_control::HubEvent;
use crate::servers::tcp_control::{Peer, PeerDirection};

/// Handshake wire format (kept simple):
///
/// ```text
/// Client → Server:  [nonce: 32 bytes] [hmac(secret, nonce): 32 bytes] [direction: 1 byte] [udp_port: 2 bytes LE]
/// Server → Client:  [0x01 = OK | 0x00 = REJECT]
/// ```
///
/// `direction` from the *client's* perspective: 0x01 = Inbound (client
/// receives shards), 0x02 = Outbound (client sends shards), 0x03 = Both.
pub struct TcpServer {
    listener: TcpListener,
    authenticator: TokenAuthenticator,
    hub_event_tx: mpsc::Sender<HubEvent>,
}

impl TcpServer {
    pub fn new(listener: TcpListener, authenticator: TokenAuthenticator, hub_event_tx: mpsc::Sender<HubEvent>) -> Self {
        Self { listener, authenticator, hub_event_tx }
    }

    /// Run the accept loop. Spawns a task per incoming TCP connection to
    /// perform the handshake, then registers the peer with the Hub.
    pub async fn run(self) {
        let local_addr = self.listener.local_addr().map(|a| a.to_string()).unwrap_or_else(|_| "unknown".to_string());
        info!("TCP server listening on {}", local_addr);

        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    debug!("TCP connection from {}", addr);
                    let authenticator_secret = self.authenticator.secret().to_vec();
                    let hub_tx = self.hub_event_tx.clone();
                    tokio::spawn(async move {
                        match handshake_accept(stream, addr, &authenticator_secret, hub_tx.clone()).await {
                            Ok(peer) => {
                                info!("Peer {} authenticated ({})", addr, peer.direction());
                                if hub_tx.send(HubEvent::PeerConnected(peer)).await.is_err() {
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
    secret: &[u8],
    hub_event_tx: mpsc::Sender<HubEvent>,
) -> RelayResult<Peer> {
    let mut buf = [0u8; HANDSHAKE_MSG_SIZE];
    timeout(HANDSHAKE_TIMEOUT, stream.read_exact(&mut buf))
        .await
        .map_err(|_| RelayError::PeerConnection(format!("handshake read timed out for {}", addr)))?
        .map_err(|e| RelayError::PeerConnection(format!("handshake read from {}: {}", addr, e)))?;

    let nonce = &buf[0..32];
    let client_hmac = &buf[32..64];
    let direction_byte = buf[64];
    let udp_port = u16::from_le_bytes([buf[65], buf[66]]);

    // Validate HMAC(secret, nonce).
    let auth = TokenAuthenticator::new(secret.to_vec());
    let nonce_array: [u8; 32] = nonce.try_into().map_err(|_| RelayError::AuthenticationFailed("invalid nonce size".into()))?;
    let expected = auth.generate_token(&nonce_array, nonce);
    if expected.as_bytes() != client_hmac {
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

    let udp_target = SocketAddr::new(addr.ip(), udp_port);
    Ok(Peer::new(addr, direction, stream, udp_target, hub_event_tx))
}

/// Connect to a remote peer, perform the handshake, and register with Hub.
///
/// `our_direction` is from *our* perspective (Inbound = we receive shards).
/// `local_udp_port` is the port of our shared UDP socket so the remote knows
/// where to send shards.
pub async fn tcp_connect(
    remote_addr: SocketAddr,
    secret: &[u8],
    our_direction: PeerDirection,
    local_udp_port: u16,
    hub_event_tx: &mpsc::Sender<HubEvent>,
) -> RelayResult<SocketAddr> {
    let mut stream = timeout(HANDSHAKE_TIMEOUT, TcpStream::connect(remote_addr))
        .await
        .map_err(|_| RelayError::PeerConnection(format!("connect timed out to {}", remote_addr)))?
        .map_err(|e| RelayError::PeerConnection(format!("connect to {}: {}", remote_addr, e)))?;

    // Generate cryptographically secure nonce + HMAC.
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);
    let auth = TokenAuthenticator::new(secret.to_vec());
    let token = auth.generate_token(&nonce, &nonce);

    // Direction byte: from *our* perspective.
    let direction_byte = match our_direction {
        PeerDirection::Inbound => 0x01,
        PeerDirection::Outbound => 0x02,
        PeerDirection::Both => 0x03,
    };

    let mut msg = [0u8; HANDSHAKE_MSG_SIZE];
    msg[0..32].copy_from_slice(&nonce);
    msg[32..64].copy_from_slice(token.as_bytes());
    msg[64] = direction_byte;
    msg[65..67].copy_from_slice(&local_udp_port.to_le_bytes());

    // perform the write with an explicit result type so the compiler
    // can infer the intermediate `Result` produced by `timeout`.
    let write_result: std::io::Result<()> = timeout(HANDSHAKE_TIMEOUT, stream.write_all(&msg))
        .await
        .map_err(|_| RelayError::PeerConnection(format!("handshake write timed out to {}", remote_addr)))?;
    write_result
        .map_err(|e| RelayError::PeerConnection(format!("handshake write to {}: {}", remote_addr, e)))?;

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
    let udp_target = SocketAddr::new(peer_addr.ip(), remote_addr.port());

    let peer = Peer::new(peer_addr, our_direction, stream, udp_target, hub_event_tx.clone());
    hub_event_tx
        .send(HubEvent::PeerConnected(peer))
        .await
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

        let (hub_tx, mut hub_rx) = mpsc::channel::<HubEvent>(16);

        // Server side: accept + handshake.
        let server_secret = secret.clone();
        let server_hub_tx = hub_tx.clone();
        let server_handle = tokio::spawn(async move {
            let (stream, addr) = listener.accept().await.unwrap();
            handshake_accept(stream, addr, &server_secret, server_hub_tx).await
        });

        // Client side: connect + handshake.
        let client_secret = secret.clone();
        let client_handle =
            tokio::spawn(async move { tcp_connect(server_addr, &client_secret, PeerDirection::Both, 9999, &hub_tx).await });

        // Both should succeed.
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

        let (hub_tx, _hub_rx) = mpsc::channel::<HubEvent>(16);

        let server_hub_tx = hub_tx.clone();
        let server_handle = tokio::spawn(async move {
            let (stream, addr) = listener.accept().await.unwrap();
            handshake_accept(stream, addr, b"correct-secret", server_hub_tx).await
        });

        let client_handle =
            tokio::spawn(async move { tcp_connect(server_addr, b"wrong-secret", PeerDirection::Inbound, 9999, &hub_tx).await });

        let server_result = server_handle.await.unwrap();
        assert!(server_result.is_err());

        let client_result = client_handle.await.unwrap();
        assert!(client_result.is_err());
    }
}
