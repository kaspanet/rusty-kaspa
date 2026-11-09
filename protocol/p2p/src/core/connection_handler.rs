use crate::common::ProtocolError;
use crate::core::hub::HubEvent;
use crate::libp2p_transport::{ConnectionHandle, Libp2pStream, Libp2pTransport};
use crate::pb::{
    KaspadMessage, p2p_client::P2pClient as ProtoP2pClient, p2p_server::P2p as ProtoP2p, p2p_server::P2pServer as ProtoP2pServer,
};
use crate::{ConnectionInitializer, Router};
use futures::FutureExt;
use kaspa_core::{debug, info, warn};
use kaspa_utils::{connection::Libp2pId, networking::NetAddress};
use kaspa_utils_tower::{
    counters::TowerConnectionCounters,
    middleware::{CountBytesBody, MapRequestBodyLayer, MapResponseBodyLayer, ServiceBuilder},
};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc::{Sender as MpscSender, channel as mpsc_channel};
use tokio::sync::oneshot::{Sender as OneshotSender, channel as oneshot_channel};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Error as TonicError, Server as TonicServer};
use tonic::{Request, Response, Status as TonicStatus, Streaming};

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("local connection capacity exhausted")]
    AtCapacity,

    #[error("missing socket address")]
    NoAddress,

    #[error(transparent)]
    Libp2p(#[from] crate::libp2p_transport::TransportError),

    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    TonicError(#[from] TonicError),

    #[error("{0}")]
    TonicStatus(#[from] TonicStatus),

    #[error("{0}")]
    ProtocolError(#[from] ProtocolError),
}

/// Maximum P2P decoded gRPC message size to send and receive
const P2P_MAX_MESSAGE_SIZE: usize = 256 * 1024 * 1024; // 256MB

const MAX_PENDING_INBOUND_HANDSHAKES: usize = 32;
const MAX_PENDING_INBOUND_HANDSHAKES_PER_IP: usize = 2;

#[derive(Debug, Default)]
struct PendingInboundHandshakes {
    total: usize,
    by_ip: HashMap<IpAddr, usize>,
}

impl PendingInboundHandshakes {
    fn try_reserve(&mut self, ip: IpAddr) -> bool {
        let ip_count = self.by_ip.get(&ip).copied().unwrap_or_default();
        if self.total >= MAX_PENDING_INBOUND_HANDSHAKES || ip_count >= MAX_PENDING_INBOUND_HANDSHAKES_PER_IP {
            return false;
        }

        #[allow(clippy::arithmetic_side_effects, reason = "The preceding checks bound these values by 32 and 2.")]
        {
            self.total += 1;
            self.by_ip.insert(ip, ip_count + 1);
        }
        true
    }

    fn release(&mut self, ip: IpAddr) {
        self.total = self.total.checked_sub(1).expect("a pending inbound handshake reservation must exist");
        let remove = {
            let count = self.by_ip.get_mut(&ip).expect("a pending inbound handshake IP reservation must exist");
            *count = count.checked_sub(1).expect("a pending inbound handshake IP reservation must exist");
            *count == 0
        };
        if remove {
            self.by_ip.remove(&ip);
        }
    }
}

struct PendingInboundHandshakeGuard {
    pending: Arc<Mutex<PendingInboundHandshakes>>,
    ip: IpAddr,
}

impl PendingInboundHandshakeGuard {
    fn try_new(pending: Arc<Mutex<PendingInboundHandshakes>>, ip: IpAddr) -> Option<Self> {
        if pending.lock().try_reserve(ip) { Some(Self { pending, ip }) } else { None }
    }
}

impl Drop for PendingInboundHandshakeGuard {
    fn drop(&mut self) {
        self.pending.lock().release(self.ip);
    }
}

/// Carries authenticated connection metadata and handshake controls into tonic request extensions.
#[derive(Clone)]
pub(crate) struct Libp2pConnectInfo {
    /// Verified direct QUIC socket, never the relay socket.
    pub address: SocketAddr,
    /// Identity authenticated by the established direct connection.
    pub id: Libp2pId,
    /// Signals that the peer opened the Kaspa RPC before its HTTP/2 startup deadline.
    pub request_started: tokio_util::sync::CancellationToken,
    /// Closes the transport connection when the Kaspa session ends.
    pub connection: ConnectionHandle,
}

/// Handles Router creation for both server and client-side new connections
#[derive(Clone)]
pub struct ConnectionHandler {
    /// Cloned on each new connection so that routers can communicate with a central hub
    hub_sender: MpscSender<HubEvent>,
    initializer: Arc<dyn ConnectionInitializer>,
    counters: Arc<TowerConnectionCounters>,
    pub(crate) libp2p: Arc<Libp2pTransport>,
    pending_inbound_handshakes: Arc<Mutex<PendingInboundHandshakes>>,
}

impl ConnectionHandler {
    pub(crate) fn new(
        hub_sender: MpscSender<HubEvent>,
        initializer: Arc<dyn ConnectionInitializer>,
        counters: Arc<TowerConnectionCounters>,
        libp2p: Arc<Libp2pTransport>,
    ) -> Self {
        Self { hub_sender, libp2p, initializer, counters, pending_inbound_handshakes: Default::default() }
    }

    /// Launches a P2P server listener loop
    pub(crate) fn serve(&self, serve_address: NetAddress) -> Result<OneshotSender<()>, ConnectionError> {
        let (termination_sender, termination_receiver) = oneshot_channel::<()>();
        let connection_handler = self.clone();
        let incoming = tonic::transport::server::TcpIncoming::bind(serve_address.into())?.with_nodelay(Some(true));
        info!("P2P Server starting on: {}", serve_address);

        tokio::spawn(async move {
            let serve_result = connection_handler.serve_incoming(incoming, termination_receiver.map(drop)).await;

            match serve_result {
                Ok(_) => {
                    info!("P2P Server stopped: {}", serve_address);
                }
                Err(err) => panic!("P2P, Server {serve_address} stopped with error: {err:?}"),
            }
        });
        Ok(termination_sender)
    }

    async fn serve_incoming<I, IO, F>(&self, incoming: I, shutdown: F) -> Result<(), TonicError>
    where
        I: futures::Stream<Item = Result<IO, std::io::Error>>,
        IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + tonic::transport::server::Connected + Unpin + Send + 'static,
        F: std::future::Future<Output = ()>,
    {
        let bytes_tx = self.counters.bytes_tx.clone();
        let bytes_rx = self.counters.bytes_rx.clone();
        let proto_server = ProtoP2pServer::new(self.clone())
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip)
            .max_decoding_message_size(P2P_MAX_MESSAGE_SIZE);
        TonicServer::builder()
            .layer(MapRequestBodyLayer::new(move |body| tonic::body::Body::new(CountBytesBody::new(body, bytes_rx.clone()))))
            .layer(MapResponseBodyLayer::new(move |body| tonic::body::Body::new(CountBytesBody::new(body, bytes_tx.clone()))))
            .add_service(proto_server)
            .serve_with_incoming_shutdown(incoming, shutdown)
            .await
    }

    /// Connect to a new peer
    pub(crate) async fn connect(&self, peer_address: String) -> Result<Arc<Router>, ConnectionError> {
        let Some(socket_address) = peer_address.to_socket_addrs()?.next() else {
            return Err(ConnectionError::NoAddress);
        };
        let peer_address = format!("http://{}", peer_address); // Add scheme prefix as required by Tonic

        let channel = tonic::transport::Endpoint::new(peer_address)?
            .timeout(Duration::from_millis(Self::communication_timeout()))
            .connect_timeout(Duration::from_millis(Self::connect_timeout()))
            .tcp_keepalive(Some(Duration::from_millis(Self::keep_alive())))
            .connect()
            .await?;

        self.connect_channel(channel, socket_address, None).await
    }

    async fn connect_channel(
        &self,
        channel: tonic::transport::Channel,
        address: SocketAddr,
        libp2p_id: Option<Libp2pId>,
    ) -> Result<Arc<Router>, ConnectionError> {
        let channel = ServiceBuilder::new()
            .layer(MapResponseBodyLayer::new(move |body| {
                tonic::body::Body::new(CountBytesBody::new(body, self.counters.bytes_rx.clone()))
            }))
            .layer(MapRequestBodyLayer::new(move |body| {
                tonic::body::Body::new(CountBytesBody::new(body, self.counters.bytes_tx.clone()))
            }))
            .service(channel);

        let mut client = ProtoP2pClient::new(channel)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip)
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
            .max_decoding_message_size(P2P_MAX_MESSAGE_SIZE);

        let (outgoing_route, outgoing_receiver) = mpsc_channel(Self::outgoing_network_channel_size());
        let incoming_stream = client.message_stream(ReceiverStream::new(outgoing_receiver)).await?.into_inner();

        let router = Router::new(address, libp2p_id, true, self.hub_sender.clone(), incoming_stream, outgoing_route).await;

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

    pub(crate) async fn connect_libp2p(
        &self,
        record: &kaspa_utils::connection::EndpointAddress,
    ) -> Result<Arc<Router>, ConnectionError> {
        let stream = self.libp2p.connect(record).await?;
        let address = stream.info.address;
        let id = stream.info.id;
        let guard = crate::libp2p_transport::SessionCloseGuard(stream.info.connection.clone());
        let mut slot = Some(stream);
        let channel = tonic::transport::Endpoint::from_static("http://kaspa.libp2p")
            .timeout(Duration::from_millis(Self::communication_timeout()))
            .connect_timeout(Duration::from_secs(10))
            .connect_with_connector(tower::service_fn(move |_| {
                let stream = slot.take();
                async move {
                    stream
                        .map(hyper_util::rt::TokioIo::new)
                        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Libp2p stream already consumed"))
                }
            }))
            .await?;
        let router = self.connect_channel(channel, address, Some(id)).await?;
        // Release the outbound session and its quota when the router closes,
        // even if tonic keeps its idle HTTP/2 connection alive.
        let closed = router.closed();
        let transport = self.libp2p.clone();
        self.libp2p.spawn(async move {
            let _guard = guard;
            tokio::select! {
                _ = closed => {},
                _ = transport.cancelled() => {},
            }
        });
        Ok(router)
    }

    /// Serves an inbound Kaspa gRPC session over an authenticated libp2p stream.
    pub(crate) async fn serve_libp2p(
        &self,
        stream: Libp2pStream,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<(), ConnectionError> {
        let info = stream.info.clone();
        // EOF on the acceptor makes tonic gracefully shut down its HTTP/2
        // connections. Keep this one-connection acceptor pending until the
        // authenticated connection or daemon is cancelled.
        let incoming = tokio_stream::once(Ok::<_, std::io::Error>(stream)).chain(futures::stream::pending());
        let serving = self.serve_incoming(incoming, cancel.cancelled_owned());
        tokio::pin!(serving);
        let started = async {
            tokio::select! {
                result = &mut serving => result?,
                _ = info.request_started.cancelled() => serving.await?,
                _ = tokio::time::sleep(Duration::from_secs(10)) => {
                    info.connection.close();
                    return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Kaspa HTTP/2 session deadline").into());
                }
            }
            Ok::<_, ConnectionError>(())
        };
        tokio::select! {
            _ = info.connection.cancel.cancelled() => {},
            _ = self.libp2p.cancelled() => {},
            result = started => result?,
        }
        Ok(())
    }

    /// Connect to a new peer with `retry_attempts` retries and `retry_interval` duration between each attempt
    pub(crate) async fn connect_with_retry(
        &self,
        address: String,
        retry_attempts: u8,
        retry_interval: Duration,
    ) -> Result<Arc<Router>, ConnectionError> {
        for counter in 1u16.. {
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
                    if counter < retry_attempts as u16 {
                        // Await `retry_interval` time before retrying
                        tokio::time::sleep(retry_interval).await;
                    } else {
                        debug!("P2P, Client connection retry #{} - all failed", retry_attempts);
                        return Err(err);
                    }
                }
            }
        }
        unreachable!()
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
        let libp2p_info = request.extensions().get::<Libp2pConnectInfo>().cloned();
        let remote_address = libp2p_info
            .as_ref()
            .map(|info| info.address)
            .or_else(|| request.remote_addr())
            .ok_or_else(|| TonicStatus::invalid_argument("Incoming connection opening request has no remote address"))?;
        let remote_ip = remote_address.ip().to_canonical();
        // Bound pending handshakes globally and per source IP before allocating the router and its channels.
        let inbound_handshake_guard = PendingInboundHandshakeGuard::try_new(self.pending_inbound_handshakes.clone(), remote_ip)
            .ok_or_else(|| TonicStatus::resource_exhausted("too many pending inbound handshakes"))?;

        // Signal that the RPC has started, ending serve_libp2p's ten-second startup deadline.
        if let Some(info) = &libp2p_info {
            info.request_started.cancel();
        }
        // Build the in/out pipes
        let (outgoing_route, outgoing_receiver) = mpsc_channel(Self::outgoing_network_channel_size());
        let incoming_stream = request.into_inner();

        // Build the router object
        let libp2p_id = libp2p_info.as_ref().map(|info| info.id);
        let router = Router::new(remote_address, libp2p_id, false, self.hub_sender.clone(), incoming_stream, outgoing_route).await;

        let initializer = self.initializer.clone();
        let hub_sender = self.hub_sender.clone();
        tokio::spawn(async move {
            // Hold the reservation until initialization and any failure cleanup complete.
            let _inbound_handshake_guard = inbound_handshake_guard;
            match initializer.initialize_connection(router.clone()).await {
                Ok(()) => {
                    hub_sender.send(HubEvent::NewPeer(router)).await.expect("hub receiver should never drop before senders");
                }
                Err(err) => {
                    router.try_sending_reject_message(&err).await;
                    router.close().await;

                    match err {
                        ProtocolError::LoopbackConnection(_)
                        | ProtocolError::PeerAlreadyExists(_)
                        | ProtocolError::VersionMismatch(_, ..=9) => {
                            // version 9 and below is prior toccata, silencing logs on deprecated versions
                            debug!("P2P, handshake failed for inbound peer {}: {}", router, err);
                        }
                        _ => {
                            warn!("P2P, handshake failed for inbound peer {}: {}", router, err);
                        }
                    }
                }
            }
        });

        // Give tonic a receiver stream (messages sent to it will be forwarded to the network peer)
        let guard = libp2p_info.map(|info| crate::libp2p_transport::SessionCloseGuard(info.connection));
        let stream: Self::MessageStreamStream = Box::pin(ReceiverStream::new(outgoing_receiver).map(move |message| {
            // Keep the optional guard until this response stream is dropped, then close the libp2p connection and release its quota.
            let _guard = &guard;
            Ok(message)
        }));
        Ok(Response::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn pending_inbound_handshake_limits_and_reuses_slots() {
        let pending = Arc::new(Mutex::new(PendingInboundHandshakes::default()));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

        let first = PendingInboundHandshakeGuard::try_new(pending.clone(), ip).unwrap();
        let second = PendingInboundHandshakeGuard::try_new(pending.clone(), ip).unwrap();
        assert!(PendingInboundHandshakeGuard::try_new(pending.clone(), ip).is_none());

        drop(first);
        let replacement = PendingInboundHandshakeGuard::try_new(pending.clone(), ip).unwrap();
        drop((second, replacement));

        let mut guards = Vec::with_capacity(MAX_PENDING_INBOUND_HANDSHAKES);
        for i in 0..MAX_PENDING_INBOUND_HANDSHAKES {
            let ip = IpAddr::V6(Ipv6Addr::from(u128::try_from(i).unwrap()));
            guards.push(PendingInboundHandshakeGuard::try_new(pending.clone(), ip).unwrap());
        }
        let next_ip = IpAddr::V6(Ipv6Addr::from(u128::try_from(MAX_PENDING_INBOUND_HANDSHAKES).unwrap()));
        assert!(PendingInboundHandshakeGuard::try_new(pending.clone(), next_ip).is_none());

        drop(guards);
        let pending = pending.lock();
        assert_eq!(pending.total, 0);
        assert!(pending.by_ip.is_empty());
    }

    #[tokio::test]
    async fn pending_inbound_handshake_is_released_on_task_abort() {
        let pending = Arc::new(Mutex::new(PendingInboundHandshakes::default()));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let guard = PendingInboundHandshakeGuard::try_new(pending.clone(), ip).unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();

        let task = tokio::spawn(async move {
            let _guard = guard;
            started_tx.send(()).unwrap();
            futures::future::pending::<()>().await;
        });
        started_rx.await.unwrap();

        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());

        let pending = pending.lock();
        assert_eq!(pending.total, 0);
        assert!(pending.by_ip.is_empty());
    }
}
