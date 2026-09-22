use crate::common::ProtocolError;
use crate::core::hub::HubEvent;
use crate::pb::{
    KaspadMessage, p2p_client::P2pClient as ProtoP2pClient, p2p_server::P2p as ProtoP2p, p2p_server::P2pServer as ProtoP2pServer,
};
use crate::{ConnectionInitializer, Router};
use futures::FutureExt;
use kaspa_core::{debug, info, warn};
use kaspa_utils::networking::NetAddress;
use kaspa_utils_tower::{
    counters::TowerConnectionCounters,
    middleware::{CountBytesBody, MapRequestBodyLayer, MapResponseBodyLayer, ServiceBuilder},
};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::{IpAddr, ToSocketAddrs};
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
    #[error("missing socket address")]
    NoAddress,

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

/// Handles Router creation for both server and client-side new connections
#[derive(Clone)]
pub struct ConnectionHandler {
    /// Cloned on each new connection so that routers can communicate with a central hub
    hub_sender: MpscSender<HubEvent>,
    initializer: Arc<dyn ConnectionInitializer>,
    counters: Arc<TowerConnectionCounters>,
    pending_inbound_handshakes: Arc<Mutex<PendingInboundHandshakes>>,
}

impl ConnectionHandler {
    pub(crate) fn new(
        hub_sender: MpscSender<HubEvent>,
        initializer: Arc<dyn ConnectionInitializer>,
        counters: Arc<TowerConnectionCounters>,
    ) -> Self {
        Self { hub_sender, initializer, counters, pending_inbound_handshakes: Default::default() }
    }

    /// Launches a P2P server listener loop
    pub(crate) fn serve(&self, serve_address: NetAddress) -> Result<OneshotSender<()>, ConnectionError> {
        let (termination_sender, termination_receiver) = oneshot_channel::<()>();
        let connection_handler = self.clone();
        info!("P2P Server starting on: {}", serve_address);

        let bytes_tx = self.counters.bytes_tx.clone();
        let bytes_rx = self.counters.bytes_rx.clone();

        tokio::spawn(async move {
            let proto_server = ProtoP2pServer::new(connection_handler)
                .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
                .send_compressed(tonic::codec::CompressionEncoding::Gzip)
                .max_decoding_message_size(P2P_MAX_MESSAGE_SIZE);

            // TODO: check whether we should set tcp_keepalive
            let serve_result = TonicServer::builder()
                .layer(MapRequestBodyLayer::new(move |body| tonic::body::Body::new(CountBytesBody::new(body, bytes_rx.clone()))))
                .layer(MapResponseBodyLayer::new(move |body| tonic::body::Body::new(CountBytesBody::new(body, bytes_tx.clone()))))
                .add_service(proto_server)
                .serve_with_shutdown(serve_address.into(), termination_receiver.map(drop))
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

        let router = Router::new(socket_address, true, self.hub_sender.clone(), incoming_stream, outgoing_route).await;

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
        let Some(remote_address) = request.remote_addr() else {
            return Err(TonicStatus::new(tonic::Code::InvalidArgument, "Incoming connection opening request has no remote address"));
        };

        // Bound pending handshakes globally and per source IP before allocating the router and its channels.
        let inbound_handshake_guard =
            PendingInboundHandshakeGuard::try_new(self.pending_inbound_handshakes.clone(), remote_address.ip().to_canonical())
                .ok_or_else(|| TonicStatus::resource_exhausted("too many pending inbound handshakes"))?;

        // Build the in/out pipes
        let (outgoing_route, outgoing_receiver) = mpsc_channel(Self::outgoing_network_channel_size());
        let incoming_stream = request.into_inner();

        // Build the router object
        let router = Router::new(remote_address, false, self.hub_sender.clone(), incoming_stream, outgoing_route).await;

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
        Ok(Response::new(Box::pin(ReceiverStream::new(outgoing_receiver).map(Ok)) as Self::MessageStreamStream))
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
