use crate::pb;
use crate::pb::KaspadMessage;
use fmt::Debug;
use futures::FutureExt;
use kaspa_core::{debug, error, info, trace, warn};
use lockfree;
use std::fmt;
use std::{error::Error, net::ToSocketAddrs, pin::Pin, result::Result};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::codec::CompressionEncoding;
use tonic::{async_trait, transport::Server, Status};
use uuid;

#[allow(dead_code)]
fn match_for_io_error(err_status: &Status) -> Option<&std::io::Error> {
    let mut err: &(dyn Error + 'static) = err_status;

    loop {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err);
        }

        // h2::Error do not expose std::io::Error with `source()`
        // https://github.com/hyperium/h2/pull/462
        if let Some(h2_err) = err.downcast_ref::<h2::Error>() {
            if let Some(io_err) = h2_err.get_io() {
                return Some(io_err);
            }
        }

        err = match err.source() {
            Some(err) => err,
            None => return None,
        };
    }
}

#[async_trait]
pub trait RouterApi: Send + Sync + 'static {
    // expected to be called once
    async fn new() -> (std::sync::Arc<Self>, mpsc::Receiver<std::sync::Arc<Self>>);
    // expected to be called from grpc service (server & client)
    async fn clone(
        &self,
        server_sender: Option<mpsc::Sender<Result<pb::KaspadMessage, tonic::Status>>>,
        client_sender: Option<mpsc::Sender<pb::KaspadMessage>>,
    ) -> std::sync::Arc<Self>;

    async fn route_to_flow(&self, msg: pb::KaspadMessage) -> bool;
    async fn route_to_network(&self, msg: pb::KaspadMessage) -> bool;
    async fn broadcast(&self, msg: pb::KaspadMessage) -> bool;
    async fn reroute_to_flow(&self);
    async fn finalize(&self);
    #[allow(clippy::wrong_self_convention)]
    fn from_network_buffer_size(&self) -> usize;
    fn to_network_buffer_size(&self) -> usize;
    // mapping function
    fn grpc_payload_to_internal_u8_enum(payload: &pb::kaspad_message::Payload) -> u8;
    // expected to be called by upper layer to register msg-types of interest
    fn subscribe_to(&self, msgs: std::vec::Vec<KaspadMessagePayloadEnumU8>) -> mpsc::Receiver<pb::KaspadMessage>;
    // Management
    async fn close(&self);
    // Identity
    fn identity(&self) -> uuid::Uuid;
}

#[derive(Debug)]
pub struct GrpcConnection<T: RouterApi> {
    router: std::sync::Arc<T>,
}

#[tonic::async_trait]
impl<T: RouterApi> pb::p2p_server::P2p for GrpcConnection<T> {
    type MessageStreamStream = Pin<Box<dyn futures::Stream<Item = Result<pb::KaspadMessage, tonic::Status>> + Send + 'static>>;
    async fn message_stream(
        &self,
        request: tonic::Request<tonic::Streaming<pb::KaspadMessage>>,
    ) -> Result<tonic::Response<Self::MessageStreamStream>, tonic::Status> {
        trace!("P2P, Server - at new connection");
        // [0] - Create channel for sending messages to the network by upper-layer, p2p layer will use its pop-end
        let (push_msg_send_by_us_to_network, pop_msg_send_by_us_to_network) =
            mpsc::channel::<Result<pb::KaspadMessage, tonic::Status>>(self.router.to_network_buffer_size());
        // [1] - Register & notify ...
        let router = self.router.as_ref().clone(Some(push_msg_send_by_us_to_network), None).await;
        // [2] - dispatch loop - exits when no-route exists or channel is closed
        tokio::spawn(async move {
            trace!("P2P, Server at receiving loop start");
            let mut input_from_network_grpc_stream = request.into_inner();
            while let Some(result) = input_from_network_grpc_stream.next().await {
                match result {
                    Ok(msg) => {
                        trace!("P2P, Server - got message: {:?}, router-id: {}", msg, router.identity());
                        // if it is false -> no route for message exists or channel is closed / dropped
                        if !(router.route_to_flow(msg).await) {
                            trace!(
                                "P2P, Server - no route exist for this message, going to close connection, router-id: {}",
                                router.identity()
                            );
                            router.close().await;
                            break;
                        }
                    }
                    Err(err) => {
                        warn!("P2P, server side: got error: {:?}", err);
                        router.close().await;
                        break;
                    }
                }
            }
        });
        // [3] - map stream to be send to the network, dispatch will be handled by grpc
        trace!("P2P, Server at final stage of grpc connection registration");
        Ok(tonic::Response::new(Box::pin(ReceiverStream::new(pop_msg_send_by_us_to_network)) as Self::MessageStreamStream))
    }
}

pub struct P2pServer;

impl P2pServer {
    pub async fn listen<T: RouterApi>(
        ip_port: String,
        router: std::sync::Arc<T>,
        gzip: bool,
    ) -> Result<tokio::sync::oneshot::Sender<()>, tonic::transport::Error> {
        info!("P2P, Start Listener, ip & port: {:?}", ip_port);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            debug!("P2P, Listener starting, ip & port: {:?} [blocking call]....", ip_port);
            let grpc_server = match gzip {
                true => pb::p2p_server::P2pServer::new(GrpcConnection { router })
                    .accept_compressed(CompressionEncoding::Gzip)
                    .send_compressed(CompressionEncoding::Gzip),
                false => pb::p2p_server::P2pServer::new(GrpcConnection { router }),
            };

            Server::builder()
                    //.add_service(pb::p2p_server::P2pServer::new(GrpcConnection { router }))
                    .add_service(grpc_server)
                    .serve_with_shutdown(
                        ip_port.to_socket_addrs().unwrap().next().unwrap(),
                        rx.map(drop),
                        //async {
                        //let _ = tokio::spawn(tokio::signal::ctrl_c()).await.unwrap();
                        //}
                    )
                    .await
                    .unwrap();
            debug!("P2P, Listener stopped, ip & port: {:?}", ip_port);
        });
        Ok(tx)
    }
}

pub struct P2pClient<T: RouterApi> {
    grpc_client: Option<pb::p2p_client::P2pClient<tonic::transport::Channel>>,
    pub router: std::sync::Arc<T>,
}

impl<T: RouterApi> P2pClient<T> {
    #[allow(dead_code)]
    pub async fn connect(
        address: String,
        router: std::sync::Arc<T>,
        gzip: bool,
    ) -> std::result::Result<Self, tonic::transport::Error> {
        // [0] - Connection
        debug!("P2P, P2pClient::connect - ip & port: {}", address.clone());
        let channel = tonic::transport::Endpoint::new(address.clone())?
            .timeout(tokio::time::Duration::from_millis(P2pClient::<T>::communication_timeout()))
            .connect_timeout(tokio::time::Duration::from_millis(P2pClient::<T>::connect_timeout()))
            .tcp_keepalive(Some(tokio::time::Duration::from_millis(P2pClient::<T>::keep_alive())))
            .connect()
            .await?;

        // [1] - channels
        let (tx, rx) = mpsc::channel(router.from_network_buffer_size());
        // [2] - Wrapped client
        trace!("P2P, P2pClient::connect - grpc client creation ...");
        let mut p2p_client = match gzip {
            true => P2pClient {
                grpc_client: Some(
                    // TODO: if new is failed ?
                    pb::p2p_client::P2pClient::new(channel)
                        .send_compressed(tonic::codec::CompressionEncoding::Gzip)
                        .accept_compressed(tonic::codec::CompressionEncoding::Gzip),
                ),
                router: router.as_ref().clone(None, Some(tx)).await,
            },
            false => P2pClient {
                grpc_client: Some(pb::p2p_client::P2pClient::new(channel)),
                router: router.as_ref().clone(None, Some(tx)).await,
            },
        };
        // [3] - Read messages from server & route to flows
        let mut input_from_network_grpc_stream =
            p2p_client.grpc_client.as_mut().unwrap().message_stream(ReceiverStream::new(rx)).await.unwrap().into_inner();
        let router_to_move = p2p_client.router.clone();
        tokio::spawn(async move {
            trace!("P2P, P2pClient::connect - receiving loop started");
            // [0] - endless loop
            // TODO: check if dropped, is await will return ?
            while let Some(result) = input_from_network_grpc_stream.next().await {
                match result {
                    Ok(msg) => {
                        trace!(
                            "P2P, P2pClient::connect - client side: got message: {:?}, router-id: {}",
                            msg,
                            router_to_move.identity()
                        );
                        // if it is false -> no route for message exists
                        if !(router_to_move.route_to_flow(msg).await) {
                            // 1) - no route for this message type exist
                            // 2) - maybe channel is dropped
                            debug!("P2P, P2pClient::connect - receiving loop will be stopped, got message that can't be routed, router-id: {}", router_to_move.identity());
                            break;
                        }
                    }
                    Err(err) => {
                        warn!("P2P, P2pClient::connect - receiving loop got error: {:?}", err);
                        break;
                    }
                }
            }
            // [1] - close
            trace!("P2P, P2pClient::connect - before close");
            router_to_move.close().await;
        });
        // [4] -  return client handle (wraps grpc-client and router instance)

        Ok(p2p_client)
    }

    pub async fn connect_with_retry(address: String, router: std::sync::Arc<T>, gzip: bool, retry: u8) -> Option<P2pClient<T>> {
        let mut cnt = 0;
        loop {
            let client = P2pClient::connect(address.clone(), router.clone(), gzip).await;
            if client.is_ok() {
                debug!("P2P, Client connected, ip & port: {:?}", address);
                return Some(client.unwrap());
            } else {
                warn!("{:?}", client.err());
                if cnt > retry {
                    warn!("P2P, Client connection re-try #{} - all failed", cnt);
                    return None;
                } else {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
                cnt += 1;
            }
        }
    }

    #[inline]
    fn communication_timeout() -> u64 {
        10_000
    }
    #[inline]
    fn keep_alive() -> u64 {
        10_000
    }
    #[inline]
    fn connect_timeout() -> u64 {
        10_000
    }
}

#[derive(Debug)]

pub struct Router {
    // its lock free in order to accept upper layer to update routing on the fly
    routing_map: lockfree::map::Map<u8, mpsc::Sender<pb::KaspadMessage>>, // TODO: wrap in `Option` since master router does not need it
    // this is main channel used to send message to the infra layer
    server_sender: Option<mpsc::Sender<Result<pb::KaspadMessage, Status>>>,
    client_sender: Option<mpsc::Sender<pb::KaspadMessage>>,
    // upper layer notification channel
    upper_layer_notification: Option<mpsc::Sender<std::sync::Arc<Router>>>,
    // default routing channels till registration of routes is finished
    default_route: (Option<lockfree::queue::Queue<pb::KaspadMessage>>, std::sync::atomic::AtomicBool),
    // identity - used for debug
    identity: uuid::Uuid,
    // broadcast rx
    // broadcast_receiver: tokio::sync::broadcast::Receiver<pb::KaspadMessage>,
    // broadcast tx
    broadcast_sender: tokio::sync::broadcast::Sender<pb::KaspadMessage>,
}

#[async_trait]
impl RouterApi for Router {
    async fn new() -> (std::sync::Arc<Self>, mpsc::Receiver<std::sync::Arc<Self>>) {
        // [0] - ctor + channels
        debug!("P2P, Router::new - master router creation");
        // [1] - broadcast tx,rx - new rx created only once for master router
        let (b_tx, mut b_rx) = tokio::sync::broadcast::channel(1024);
        let (tx, rx) = mpsc::channel(128);
        let ret_val = (
            std::sync::Arc::new(Router {
                routing_map: lockfree::map::Map::new(),
                server_sender: None,
                client_sender: None,
                upper_layer_notification: Some(tx),
                default_route: (None, std::sync::atomic::AtomicBool::new(false)),
                identity: uuid::Uuid::new_v4(),
                // broadcast_receiver: b_rx,
                broadcast_sender: b_tx,
            }),
            rx,
        );
        // [2] - endless loop
        // NOTE: this loop does not hold router-arc since if it will hold it, it will never exit !
        // Since this loop only hold Receiver channel it will exit once all Senders are dropped
        // This loop does not route messages to the network since master router not connected to anyone
        // But we still need to receive messages since in the case of broadcast channel every Receiver
        // MUST dispatch his copy of message, otherwise this message will be stacked inside channel
        // and special error handling will need to be implemented
        let master_router_id = ret_val.0.identity();
        tokio::spawn(async move {
            trace!("P2P, master router broadcast loop starting, master-router-id: {:?}", master_router_id);
            loop {
                match b_rx.recv().await {
                    Ok(msg) => {
                        trace!("P2P, master router broadcast loop, message: {:?}", msg);
                        // result is ignored since master not connected to anyone
                    }
                    Err(_err) => {
                        trace!("P2P, master router broadcast loop shutting down");
                        break;
                    }
                }
            }
        });
        // [3] - result
        ret_val
    }

    async fn clone(
        &self,
        server_sender: Option<mpsc::Sender<Result<pb::KaspadMessage, tonic::Status>>>,
        client_sender: Option<mpsc::Sender<pb::KaspadMessage>>,
    ) -> std::sync::Arc<Self> {
        // [0] - create, TODO: maybe refactor ugly static + unsafe
        let router = std::sync::Arc::new(Router {
            routing_map: lockfree::map::Map::new(),
            server_sender,
            client_sender,
            upper_layer_notification: Some(self.upper_layer_notification.as_ref().unwrap().clone()),
            default_route: (Some(lockfree::queue::Queue::new()), std::sync::atomic::AtomicBool::new(true)),
            identity: uuid::Uuid::new_v4(),
            // broadcast_receiver: self.broadcast_sender.subscribe(),
            broadcast_sender: self.broadcast_sender.clone(),
        });
        // [1] - start broadcast loop
        // NOTE: this loop will exist or during all sub-system shutdown or when route_to_network will fail
        // route_to_network can fail cause:
        // 1) disconnection
        // 2) router.close() since tx-channel will be downgraded
        let same_new_router = router.clone();
        let mut b_rx = self.broadcast_sender.subscribe();
        tokio::spawn(async move {
            trace!("P2P, router broadcast loop starting, router-id: {}", same_new_router.identity());
            loop {
                let result = b_rx.recv().await;
                match result {
                    Ok(msg) => {
                        if !(same_new_router.route_to_network(msg).await) {
                            // it is ok not to warn/error here
                            trace!("P2P, router broadcast to network loop, unable to route message to network, router-id: {}, will exit broadcast loop",same_new_router.identity());
                            break;
                        }
                    }
                    Err(_err) => {
                        trace!("P2P, router broadcast loop shutting down");
                        break;
                    }
                }
            }
        });
        // [2] - notify upper layer about new connection (TODO: what is upper layer drops all TXs ?? )
        router.upper_layer_notification.as_ref().unwrap().send(router.clone()).await.unwrap();
        // [3] - return shared_ptr
        router
    }

    async fn route_to_flow(&self, msg: pb::KaspadMessage) -> bool {
        // [0] - try to router
        let key = Router::grpc_payload_to_internal_u8_enum(msg.payload.as_ref().unwrap());
        match self.routing_map.get(&key) {
            // [1] - regular route
            Some(send_channel) => send_channel.val().send(msg).await.is_ok(),
            None => {
                // [2] - try default route if not closed yet
                if self.default_route.1.load(std::sync::atomic::Ordering::Relaxed) {
                    self.default_route.0.as_ref().unwrap().push(msg);
                    true
                } else {
                    warn!("P2P, Router::route_to_flow - no route for message-type: {:?} exist", key);
                    false
                }
            }
        }
    }

    async fn route_to_network(&self, msg: pb::KaspadMessage) -> bool {
        // [0] - first try server-like routing
        match &self.server_sender {
            Some(sender) => match sender.send(Result::Ok(msg)).await {
                Ok(_r) => true,
                Err(_e) => false,
            },
            // [1] - since server sender in None -> router used for client-like routing
            None => match &self.client_sender {
                Some(sender) => match sender.send(msg).await {
                    Ok(_r) => true,
                    Err(_e) => false,
                },
                None => {
                    // log that can't route since not registered yet
                    false
                }
            },
        }
    }

    async fn broadcast(&self, msg: KaspadMessage) -> bool {
        match self.broadcast_sender.send(msg) {
            Ok(_res) => true,
            Err(_err) => {
                trace!(
                    "P2P, Router::broadcast - broadcast failed, it can happen during shutdown/initialization, router-id: {}",
                    self.identity()
                );
                false
            }
        }
    }

    async fn reroute_to_flow(&self) {
        while let Some(msg) = self.default_route.0.as_ref().unwrap().pop() {
            // this should never failed
            let _res = self.route_to_flow(msg).await;
        }
    }

    async fn finalize(&self) {
        self.default_route.1.store(false, std::sync::atomic::Ordering::Relaxed);
        self.reroute_to_flow().await;
        debug!("P2P, Router::finalize - done, router-id: {:?}", self.identity);
    }

    fn from_network_buffer_size(&self) -> usize {
        128
    }

    fn to_network_buffer_size(&self) -> usize {
        128
    }

    #[allow(clippy::let_and_return)]
    fn grpc_payload_to_internal_u8_enum(payload: &pb::kaspad_message::Payload) -> u8 {
        let result = match payload {
            pb::kaspad_message::Payload::Addresses(_) => KaspadMessagePayloadEnumU8::Addresses,
            pb::kaspad_message::Payload::Block(_) => KaspadMessagePayloadEnumU8::Block,
            pb::kaspad_message::Payload::Transaction(_) => KaspadMessagePayloadEnumU8::Transaction,
            pb::kaspad_message::Payload::BlockLocator(_) => KaspadMessagePayloadEnumU8::BlockLocator,
            pb::kaspad_message::Payload::RequestAddresses(_) => KaspadMessagePayloadEnumU8::RequestAddresses,
            pb::kaspad_message::Payload::RequestRelayBlocks(_) => KaspadMessagePayloadEnumU8::RequestRelayBlocks,
            pb::kaspad_message::Payload::RequestTransactions(_) => KaspadMessagePayloadEnumU8::RequestTransactions,
            pb::kaspad_message::Payload::IbdBlock(_) => KaspadMessagePayloadEnumU8::IbdBlock,
            pb::kaspad_message::Payload::InvRelayBlock(_) => KaspadMessagePayloadEnumU8::InvRelayBlock,
            pb::kaspad_message::Payload::InvTransactions(_) => KaspadMessagePayloadEnumU8::InvTransactions,
            pb::kaspad_message::Payload::Ping(_) => KaspadMessagePayloadEnumU8::Ping,
            pb::kaspad_message::Payload::Pong(_) => KaspadMessagePayloadEnumU8::Pong,
            pb::kaspad_message::Payload::Verack(_) => KaspadMessagePayloadEnumU8::Verack,
            pb::kaspad_message::Payload::Version(_) => KaspadMessagePayloadEnumU8::Version,
            pb::kaspad_message::Payload::TransactionNotFound(_) => KaspadMessagePayloadEnumU8::TransactionNotFound,
            pb::kaspad_message::Payload::Reject(_) => KaspadMessagePayloadEnumU8::Reject,
            pb::kaspad_message::Payload::PruningPointUtxoSetChunk(_) => KaspadMessagePayloadEnumU8::PruningPointUtxoSetChunk,
            pb::kaspad_message::Payload::RequestIbdBlocks(_) => KaspadMessagePayloadEnumU8::RequestIbdBlocks,
            pb::kaspad_message::Payload::UnexpectedPruningPoint(_) => KaspadMessagePayloadEnumU8::UnexpectedPruningPoint,
            pb::kaspad_message::Payload::IbdBlockLocator(_) => KaspadMessagePayloadEnumU8::IbdBlockLocator,
            pb::kaspad_message::Payload::IbdBlockLocatorHighestHash(_) => KaspadMessagePayloadEnumU8::IbdBlockLocatorHighestHash,
            pb::kaspad_message::Payload::RequestNextPruningPointUtxoSetChunk(_) => {
                KaspadMessagePayloadEnumU8::RequestNextPruningPointUtxoSetChunk
            }
            pb::kaspad_message::Payload::DonePruningPointUtxoSetChunks(_) => KaspadMessagePayloadEnumU8::DonePruningPointUtxoSetChunks,
            pb::kaspad_message::Payload::IbdBlockLocatorHighestHashNotFound(_) => {
                KaspadMessagePayloadEnumU8::IbdBlockLocatorHighestHashNotFound
            }
            pb::kaspad_message::Payload::BlockWithTrustedData(_) => KaspadMessagePayloadEnumU8::BlockWithTrustedData,
            pb::kaspad_message::Payload::DoneBlocksWithTrustedData(_) => KaspadMessagePayloadEnumU8::DoneBlocksWithTrustedData,
            pb::kaspad_message::Payload::RequestPruningPointAndItsAnticone(_) => {
                KaspadMessagePayloadEnumU8::RequestPruningPointAndItsAnticone
            }
            pb::kaspad_message::Payload::BlockHeaders(_) => KaspadMessagePayloadEnumU8::BlockHeaders,
            pb::kaspad_message::Payload::RequestNextHeaders(_) => KaspadMessagePayloadEnumU8::RequestNextHeaders,
            pb::kaspad_message::Payload::DoneHeaders(_) => KaspadMessagePayloadEnumU8::DoneHeaders,
            pb::kaspad_message::Payload::RequestPruningPointUtxoSet(_) => KaspadMessagePayloadEnumU8::RequestPruningPointUtxoSet,
            pb::kaspad_message::Payload::RequestHeaders(_) => KaspadMessagePayloadEnumU8::RequestHeaders,
            pb::kaspad_message::Payload::RequestBlockLocator(_) => KaspadMessagePayloadEnumU8::RequestBlockLocator,
            pb::kaspad_message::Payload::PruningPoints(_) => KaspadMessagePayloadEnumU8::PruningPoints,
            pb::kaspad_message::Payload::RequestPruningPointProof(_) => KaspadMessagePayloadEnumU8::RequestPruningPointProof,
            pb::kaspad_message::Payload::PruningPointProof(_) => KaspadMessagePayloadEnumU8::PruningPointProof,
            pb::kaspad_message::Payload::Ready(_) => KaspadMessagePayloadEnumU8::Ready,
            pb::kaspad_message::Payload::BlockWithTrustedDataV4(_) => KaspadMessagePayloadEnumU8::BlockWithTrustedDataV4,
            pb::kaspad_message::Payload::TrustedData(_) => KaspadMessagePayloadEnumU8::TrustedData,
            pb::kaspad_message::Payload::RequestIbdChainBlockLocator(_) => KaspadMessagePayloadEnumU8::RequestIbdChainBlockLocator,
            pb::kaspad_message::Payload::IbdChainBlockLocator(_) => KaspadMessagePayloadEnumU8::IbdChainBlockLocator,
            pb::kaspad_message::Payload::RequestAnticone(_) => KaspadMessagePayloadEnumU8::RequestAnticone,
            pb::kaspad_message::Payload::RequestNextPruningPointAndItsAnticoneBlocks(_) => {
                KaspadMessagePayloadEnumU8::RequestNextPruningPointAndItsAnticoneBlocks
            }
            // Default Mapping
            _ => KaspadMessagePayloadEnumU8::DefaultMaxValue,
        } as u8;
        result
    }

    fn subscribe_to(&self, msgs: std::vec::Vec<KaspadMessagePayloadEnumU8>) -> mpsc::Receiver<pb::KaspadMessage> {
        // [0] - create channels that will be use for new route
        let (tx, rx) = mpsc::channel(self.from_network_buffer_size());
        // [1] - update routes
        //while let Some(msg_type) = msgs.iter().next() {
        for msg_type in msgs {
            let msg_id = msg_type as u8;
            match self.routing_map.insert(msg_id, tx.clone()) {
                Some(prev) => {
                    // not ok, override existing route
                    let _msg_id = prev.key();
                    error!(
                        "P2P, Router::subscribe_to override already existing value:{:?}, {}, router-id: {}",
                        msg_type, _msg_id, self.identity
                    );
                    panic!("Try to subscribe to existing route");
                }
                None => {
                    // everything ok, new route registered
                    trace!("Router::subscribe_to - msg_id: {} route is registered, router-id:{:?}", msg_id, self.identity());
                }
            }
        }
        // [2] - channel that will be used by upper layer to get messages
        rx
    }

    async fn close(&self) {
        // [0] - remove -> drop
        while let Some(tx) = self.routing_map.iter().next() {
            self.routing_map.remove(tx.key());
        }
        // [1] - how to close ? lets downgrade to weak-ref
        match &self.server_sender {
            Some(sender) => {
                sender.downgrade();
            }
            None => {
                if let Some(sender) = &self.client_sender {
                    sender.downgrade();
                }
            }
        }
        // [2] - should we notify upper-layer about closing ? TODO: what if we call `close` twice
        self.upper_layer_notification.as_ref().unwrap().downgrade();
        // [3] - TODO: how to close broadcast
        // self.broadcast_sender.drop();
        // [3] - debug log
        debug!("P2P, Router::close - connection finished, router-id: {:?}", self.identity);
    }

    fn identity(&self) -> uuid::Uuid {
        self.identity
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum KaspadMessagePayloadEnumU8 {
    Addresses = 0,
    Block,
    Transaction,
    BlockLocator,
    RequestAddresses,
    RequestRelayBlocks,
    RequestTransactions,
    IbdBlock,
    InvRelayBlock,
    InvTransactions,
    Ping,
    Pong,
    Verack,
    Version,
    TransactionNotFound,
    Reject,
    PruningPointUtxoSetChunk,
    RequestIbdBlocks,
    UnexpectedPruningPoint,
    IbdBlockLocator,
    IbdBlockLocatorHighestHash,
    RequestNextPruningPointUtxoSetChunk,
    DonePruningPointUtxoSetChunks,
    IbdBlockLocatorHighestHashNotFound,
    BlockWithTrustedData,
    DoneBlocksWithTrustedData,
    RequestPruningPointAndItsAnticone,
    BlockHeaders,
    RequestNextHeaders,
    DoneHeaders,
    RequestPruningPointUtxoSet,
    RequestHeaders,
    RequestBlockLocator,
    PruningPoints,
    RequestPruningPointProof,
    PruningPointProof,
    Ready,
    BlockWithTrustedDataV4,
    TrustedData,
    RequestIbdChainBlockLocator,
    IbdChainBlockLocator,
    RequestAnticone,
    RequestNextPruningPointAndItsAnticoneBlocks,
    // Default Max Value
    DefaultMaxValue = 0xFF,
}

pub type RouterRxChannelType = tokio::sync::mpsc::Receiver<pb::KaspadMessage>;

#[ignore = "not working"]
#[tokio::test]
// this test doesn't work because client does not connect when run from test-exe (to be investigated)
async fn run_p2p_server_and_client_test() -> Result<(), Box<dyn std::error::Error>> {
    // [0] - Create new router - first instance
    // upper_layer_rx will be used to dispatch notifications about new-connections, both for client & server
    let (router, mut upper_layer_rx) = Router::new().await;
    let cloned_router_arc = router.clone();
    // [1] - Start service layer to listen when new connection is coming ( Server side )
    tokio::spawn(async move {
        // loop will exit when all sender channels will be dropped
        // --> when all routers will be dropped & grpc-service will be stopped
        while let Some(new_router) = upper_layer_rx.recv().await {
            // as en example subscribe to all message-types, in reality different flows will subscribe to different message-types
            // this channel will be owned by specific flow
            let _rx_channel = new_router.subscribe_to(vec![
                KaspadMessagePayloadEnumU8::Addresses,
                KaspadMessagePayloadEnumU8::Block,
                KaspadMessagePayloadEnumU8::Transaction,
                KaspadMessagePayloadEnumU8::BlockLocator,
                KaspadMessagePayloadEnumU8::RequestAddresses,
                KaspadMessagePayloadEnumU8::RequestRelayBlocks,
                KaspadMessagePayloadEnumU8::RequestTransactions,
                KaspadMessagePayloadEnumU8::IbdBlock,
                KaspadMessagePayloadEnumU8::InvRelayBlock,
                KaspadMessagePayloadEnumU8::InvTransactions,
                KaspadMessagePayloadEnumU8::Ping,
                KaspadMessagePayloadEnumU8::Pong,
                KaspadMessagePayloadEnumU8::Verack,
                KaspadMessagePayloadEnumU8::Version,
                KaspadMessagePayloadEnumU8::TransactionNotFound,
                KaspadMessagePayloadEnumU8::Reject,
                KaspadMessagePayloadEnumU8::PruningPointUtxoSetChunk,
                KaspadMessagePayloadEnumU8::RequestIbdBlocks,
                KaspadMessagePayloadEnumU8::UnexpectedPruningPoint,
                KaspadMessagePayloadEnumU8::IbdBlockLocator,
                KaspadMessagePayloadEnumU8::IbdBlockLocatorHighestHash,
                KaspadMessagePayloadEnumU8::RequestNextPruningPointUtxoSetChunk,
                KaspadMessagePayloadEnumU8::DonePruningPointUtxoSetChunks,
                KaspadMessagePayloadEnumU8::IbdBlockLocatorHighestHashNotFound,
                KaspadMessagePayloadEnumU8::BlockWithTrustedData,
                KaspadMessagePayloadEnumU8::DoneBlocksWithTrustedData,
                KaspadMessagePayloadEnumU8::RequestPruningPointAndItsAnticone,
                KaspadMessagePayloadEnumU8::BlockHeaders,
                KaspadMessagePayloadEnumU8::RequestNextHeaders,
                KaspadMessagePayloadEnumU8::DoneHeaders,
                KaspadMessagePayloadEnumU8::RequestPruningPointUtxoSet,
                KaspadMessagePayloadEnumU8::RequestHeaders,
                KaspadMessagePayloadEnumU8::RequestBlockLocator,
                KaspadMessagePayloadEnumU8::PruningPoints,
                KaspadMessagePayloadEnumU8::RequestPruningPointProof,
                KaspadMessagePayloadEnumU8::PruningPointProof,
                KaspadMessagePayloadEnumU8::Ready,
                KaspadMessagePayloadEnumU8::BlockWithTrustedDataV4,
                KaspadMessagePayloadEnumU8::TrustedData,
                KaspadMessagePayloadEnumU8::RequestIbdChainBlockLocator,
                KaspadMessagePayloadEnumU8::IbdChainBlockLocator,
                KaspadMessagePayloadEnumU8::RequestAnticone,
                KaspadMessagePayloadEnumU8::RequestNextPruningPointAndItsAnticoneBlocks,
            ]);
        }
    });
    // [2] - Start listener (de-facto Server side )
    let terminate_server = P2pServer::listen(String::from("[::1]:50051"), router, false).await;

    std::thread::sleep(std::time::Duration::from_secs(2));

    // [3] - Start client
    let mut cnt = 0;
    loop {
        let client = P2pClient::connect(String::from("http://[::1]:50051"), cloned_router_arc.clone(), false).await;
        if client.is_ok() {
            // client is running, we can register flows
            // router.subscribe_to(...) , but in this example spawn @ [1] will do it for every new router

            // terminate client
            println!("Client connected ... we can terminate ...");
            client.unwrap().router.as_ref().close().await;
        } else {
            println!("{:?}", client.err());
            cnt += 1;
            if cnt > 16 {
                println!("Client connected failed - 16 retries ...");
                break;
            } else {
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }
    std::thread::sleep(std::time::Duration::from_secs(2));

    // [4] - Check that server is ok
    if let Ok(t) = terminate_server {
        println!("Server is running ... & we can terminate it");
        t.send(()).unwrap();
    } else {
        println!("{:?}", terminate_server.err());
    }

    Ok(())
}
