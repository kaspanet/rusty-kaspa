use crate::common::ProtocolError;
use crate::core::hub::HubEvent;
use crate::pb::{
    p2p_client::P2pClient as ProtoP2pClient, p2p_server::P2p as ProtoP2p, p2p_server::P2pServer as ProtoP2pServer, KaspadMessage,
};
use crate::{ConnectionInitializer, Router};
use futures::FutureExt;
use hyper_util::rt::TokioIo;
use kaspa_core::{debug, info};
use kaspa_utils::networking::{NetAddress, NetAddressError};
use kaspa_utils_tower::{
    counters::TowerConnectionCounters,
    middleware::{BodyExt, CountBytesBody, MapRequestBodyLayer, MapResponseBodyLayer, ServiceBuilder},
};
use rand::{rngs::OsRng, RngCore};
use std::fmt::Write;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, OnceLock,
};
use std::time::Duration;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel as mpsc_channel, Sender as MpscSender};
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};
use tokio_socks::tcp::socks5::Socks5Stream;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::{Error as TonicError, Server as TonicServer, Uri};
use tonic::{Request, Response, Status as TonicStatus, Streaming};
use tower::service_fn;

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    TonicError(#[from] TonicError),

    #[error("{0}")]
    TonicStatus(#[from] TonicStatus),

    #[error("{0}")]
    ProtocolError(#[from] ProtocolError),

    #[error(transparent)]
    AddressParsingError(#[from] NetAddressError),
}

/// Maximum P2P decoded gRPC message size to send and receive
const P2P_MAX_MESSAGE_SIZE: usize = 1024 * 1024 * 1024; // 1GB

/// Handles Router creation for both server and client-side new connections
#[derive(Clone)]
pub struct ConnectionHandler {
    /// Cloned on each new connection so that routers can communicate with a central hub
    hub_sender: MpscSender<HubEvent>,
    initializer: Arc<dyn ConnectionInitializer>,
    counters: Arc<TowerConnectionCounters>,
    socks_proxy: Option<SocksProxyConfig>,
}

#[derive(Clone, Debug)]
pub enum SocksAuth {
    None,
    Static { username: Arc<str>, password: Arc<str> },
    Randomized,
}

#[derive(Clone, Debug)]
pub struct SocksProxyParams {
    pub address: SocketAddr,
    pub auth: SocksAuth,
}

#[derive(Clone, Default)]
pub struct SocksProxyConfig {
    pub default: Option<SocksProxyParams>,
    pub ipv4: Option<SocksProxyParams>,
    pub ipv6: Option<SocksProxyParams>,
    pub onion: Option<SocksProxyParams>,
}

impl SocksProxyConfig {
    pub fn is_empty(&self) -> bool {
        self.default.is_none() && self.ipv4.is_none() && self.ipv6.is_none() && self.onion.is_none()
    }

    pub fn entry_for(&self, address: &NetAddress) -> Option<SocksProxyParams> {
        if address.as_onion().is_some() {
            return self.onion.clone().or_else(|| self.default.clone());
        }
        if let Some(ip) = address.as_ip() {
            return match IpAddr::from(ip) {
                IpAddr::V4(_) => self.ipv4.clone().or_else(|| self.default.clone()),
                IpAddr::V6(_) => self.ipv6.clone().or_else(|| self.default.clone()),
            };
        }
        self.default.clone()
    }
}

static STREAM_ISOLATION_PREFIX: OnceLock<String> = OnceLock::new();
static STREAM_ISOLATION_COUNTER: AtomicU64 = AtomicU64::new(0);

impl ConnectionHandler {
    pub(crate) fn new(
        hub_sender: MpscSender<HubEvent>,
        initializer: Arc<dyn ConnectionInitializer>,
        counters: Arc<TowerConnectionCounters>,
        socks_proxy: Option<SocksProxyConfig>,
    ) -> Self {
        Self { hub_sender, initializer, counters, socks_proxy }
    }

    /// Launches a P2P server listener loop
    pub(crate) fn serve(&self, serve_address: NetAddress) -> Result<OneshotSender<()>, ConnectionError> {
        let (termination_sender, termination_receiver) = oneshot_channel::<()>();
        let connection_handler = self.clone();
        info!("P2P Server starting on: {}", serve_address);

        let bytes_tx = self.counters.bytes_tx.clone();
        let bytes_rx = self.counters.bytes_rx.clone();
        let serve_socket = serve_address.to_socket_addr().expect("server must bind to an IP address");

        tokio::spawn(async move {
            let proto_server = ProtoP2pServer::new(connection_handler)
                .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
                .send_compressed(tonic::codec::CompressionEncoding::Gzip)
                .max_decoding_message_size(P2P_MAX_MESSAGE_SIZE);

            // TODO: check whether we should set tcp_keepalive
            let serve_result = TonicServer::builder()
                .layer(MapRequestBodyLayer::new(move |body| CountBytesBody::new(body, bytes_rx.clone()).boxed_unsync()))
                .layer(MapResponseBodyLayer::new(move |body| CountBytesBody::new(body, bytes_tx.clone())))
                .add_service(proto_server)
                .serve_with_shutdown(serve_socket, termination_receiver.map(drop))
                .await;

            match serve_result {
                Ok(_) => info!("P2P Server stopped: {}", serve_address),
                Err(err) => panic!("P2P, Server {serve_address} stopped with error: {err:?}"),
            }
        });
        Ok(termination_sender)
    }

    /// Connect to a new peer
    pub(crate) async fn connect(&self, peer_address: String) -> Result<Arc<Router>, ConnectionError> {
        let peer_net_address = NetAddress::from_str(&peer_address)?;
        let peer_address = format!("http://{}", peer_address); // Add scheme prefix as required by Tonic

        let endpoint = tonic::transport::Endpoint::new(peer_address)?
            .timeout(Duration::from_millis(Self::communication_timeout()))
            .connect_timeout(Duration::from_millis(Self::connect_timeout()))
            .tcp_keepalive(Some(Duration::from_millis(Self::keep_alive())));

        let channel = if let Some(proxy_params) = self.socks_proxy.as_ref().and_then(|cfg| cfg.entry_for(&peer_net_address)) {
            let connector = service_fn(move |uri: Uri| {
                let proxy_params = proxy_params.clone();
                async move {
                    let host =
                        uri.host().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing host in URI"))?.to_string();
                    let port = uri.port_u16().unwrap_or(80);
                    let target = format!("{}:{}", host, port);
                    let stream = connect_via_socks(proxy_params, target).await?;
                    Ok::<_, io::Error>(TokioIo::new(stream.into_inner()))
                }
            });
            endpoint.connect_with_connector(connector).await?
        } else {
            endpoint.connect().await?
        };

        let channel = ServiceBuilder::new()
            .layer(MapResponseBodyLayer::new(move |body| CountBytesBody::new(body, self.counters.bytes_rx.clone())))
            .layer(MapRequestBodyLayer::new(move |body| CountBytesBody::new(body, self.counters.bytes_tx.clone()).boxed_unsync()))
            .service(channel);

        let mut client = ProtoP2pClient::new(channel)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip)
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
            .max_decoding_message_size(P2P_MAX_MESSAGE_SIZE);

        let (outgoing_route, outgoing_receiver) = mpsc_channel(Self::outgoing_network_channel_size());
        let incoming_stream = client.message_stream(ReceiverStream::new(outgoing_receiver)).await?.into_inner();

        let router = Router::new(peer_net_address, true, self.hub_sender.clone(), incoming_stream, outgoing_route).await;

        // For outbound peers, we perform the initialization as part of the connect logic
        match self.initializer.initialize_connection(router.clone()).await {
            Ok(()) => {
                // Notify the central Hub about the new peer
                self.hub_sender.send(HubEvent::NewPeer(router.clone())).await.expect("hub receiver should never drop before senders");
            }

            Err(err) => {
                router.try_sending_reject_message(&err).await;
                // Ignoring the new router
                router.close().await;
                debug!("P2P, handshake failed for outbound peer {}: {}", router, err);
                return Err(ConnectionError::ProtocolError(err));
            }
        }

        Ok(router)
    }

    /// Connect to a new peer with `retry_attempts` retries and `retry_interval` duration between each attempt
    pub(crate) async fn connect_with_retry(
        &self,
        address: String,
        retry_attempts: u8,
        retry_interval: Duration,
    ) -> Result<Arc<Router>, ConnectionError> {
        let mut counter = 0;
        loop {
            counter += 1;
            match self.connect(address.clone()).await {
                Ok(router) => {
                    debug!("P2P, Client connected, peer: {:?}", address);
                    return Ok(router);
                }
                Err(ConnectionError::ProtocolError(err)) => {
                    // On protocol errors we avoid retrying
                    debug!("P2P, connect retry #{} failed with error {:?}, peer: {:?}, aborting retries", counter, err, address);
                    return Err(ConnectionError::ProtocolError(err));
                }
                Err(err) => {
                    debug!("P2P, connect retry #{} failed with error {:?}, peer: {:?}", counter, err, address);
                    if counter < retry_attempts {
                        // Await `retry_interval` time before retrying
                        tokio::time::sleep(retry_interval).await;
                    } else {
                        debug!("P2P, Client connection retry #{} - all failed", retry_attempts);
                        return Err(err);
                    }
                }
            }
        }
    }

    // TODO: revisit the below constants
    fn outgoing_network_channel_size() -> usize {
        // TODO: this number is taken from go-kaspad and should be re-evaluated
        (1 << 17) + 256
    }

    fn communication_timeout() -> u64 {
        10_000
    }

    fn keep_alive() -> u64 {
        10_000
    }

    fn connect_timeout() -> u64 {
        1_000
    }
}

#[tonic::async_trait]
impl ProtoP2p for ConnectionHandler {
    type MessageStreamStream = Pin<Box<dyn futures::Stream<Item = Result<KaspadMessage, TonicStatus>> + Send + 'static>>;

    /// Handle the new arriving **server** connections
    async fn message_stream(
        &self,
        request: Request<Streaming<KaspadMessage>>,
    ) -> Result<Response<Self::MessageStreamStream>, TonicStatus> {
        let Some(remote_address) = request.remote_addr() else {
            return Err(TonicStatus::new(tonic::Code::InvalidArgument, "Incoming connection opening request has no remote address"));
        };

        // Build the in/out pipes
        let (outgoing_route, outgoing_receiver) = mpsc_channel(Self::outgoing_network_channel_size());
        let incoming_stream = request.into_inner();

        // Build the router object
        let router = Router::new(remote_address.into(), false, self.hub_sender.clone(), incoming_stream, outgoing_route).await;

        // Notify the central Hub about the new peer
        self.hub_sender.send(HubEvent::NewPeer(router)).await.expect("hub receiver should never drop before senders");

        // Give tonic a receiver stream (messages sent to it will be forwarded to the network peer)
        Ok(Response::new(Box::pin(ReceiverStream::new(outgoing_receiver).map(Ok)) as Self::MessageStreamStream))
    }
}

fn random_isolation_prefix() -> String {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    let mut prefix = String::with_capacity(bytes.len() * 2 + 1);
    for byte in &bytes {
        let _ = write!(&mut prefix, "{:02x}", byte);
    }
    prefix.push('-');
    prefix
}

fn next_stream_isolation_credentials() -> (String, String) {
    let prefix = STREAM_ISOLATION_PREFIX.get_or_init(random_isolation_prefix);
    let counter = STREAM_ISOLATION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let value = format!("{prefix}{counter}");
    (value.clone(), value)
}

async fn connect_via_socks(params: SocksProxyParams, target: String) -> io::Result<Socks5Stream<TcpStream>> {
    let address = params.address;
    let result = match params.auth {
        SocksAuth::None => Socks5Stream::connect(address, target).await,
        SocksAuth::Static { username, password } => Socks5Stream::connect_with_password(address, target, &username, &password).await,
        SocksAuth::Randomized => {
            let (username, password) = next_stream_isolation_credentials();
            Socks5Stream::connect_with_password(address, target, username.as_str(), password.as_str()).await
        }
    };
    result.map_err(|err| io::Error::new(io::ErrorKind::Other, err))
}
