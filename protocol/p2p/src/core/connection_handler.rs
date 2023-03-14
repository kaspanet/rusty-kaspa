use crate::core::hub::HubEvent;
use crate::pb::{
    p2p_client::P2pClient as ProtoP2pClient, p2p_server::P2p as ProtoP2p, p2p_server::P2pServer as ProtoP2pServer, KaspadMessage,
};
use crate::Router;
use futures::FutureExt;
use kaspa_core::{debug, info, warn};
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc::{channel as mpsc_channel, Sender as MpscSender};
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
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
}

/// Handles Router creation for both server and client-side new connections
#[derive(Clone)]
pub struct ConnectionHandler {
    /// Cloned on each new connection so that routers can communicate with a central hub
    hub_sender: MpscSender<HubEvent>,
}

impl ConnectionHandler {
    pub(crate) fn new(hub_sender: MpscSender<HubEvent>) -> Self {
        Self { hub_sender }
    }

    /// Launches a P2P server listener loop
    pub(crate) fn serve(&self, serve_address: String) -> Result<OneshotSender<()>, ConnectionError> {
        let (termination_sender, termination_receiver) = oneshot_channel::<()>();
        let connection_handler = self.clone();
        let Some(socket_address) = serve_address.to_socket_addrs()?.next() else { return Err(ConnectionError::NoAddress); };
        info!("P2P Server starting on: {}", serve_address);
        tokio::spawn(async move {
            let proto_server = ProtoP2pServer::new(connection_handler)
                .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
                .send_compressed(tonic::codec::CompressionEncoding::Gzip);

            let serve_result = TonicServer::builder()
                .add_service(proto_server)
                .serve_with_shutdown(socket_address, termination_receiver.map(drop))
                .await;

            match serve_result {
                Ok(_) => debug!("P2P, Server stopped: {}", serve_address),
                Err(err) => panic!("P2P, Server {serve_address} stopped with error: {err:?}"),
            }
        });
        Ok(termination_sender)
    }

    /// Connect to a new peer
    pub(crate) async fn connect(&self, peer_address: String) -> Result<Arc<Router>, ConnectionError> {
        let Some(socket_address) = peer_address.to_socket_addrs()?.next() else { return Err(ConnectionError::NoAddress); };
        let peer_address = format!("http://{}", peer_address); // Add scheme prefix as required by Tonic
        let channel = tonic::transport::Endpoint::new(peer_address)?
            .timeout(Duration::from_millis(Self::communication_timeout()))
            .connect_timeout(Duration::from_millis(Self::connect_timeout()))
            .tcp_keepalive(Some(Duration::from_millis(Self::keep_alive())))
            .connect()
            .await?;

        let mut client = ProtoP2pClient::new(channel)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip)
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip);

        let (outgoing_route, outgoing_receiver) = mpsc_channel(Self::outgoing_network_channel_size());
        let incoming_stream = client.message_stream(ReceiverStream::new(outgoing_receiver)).await?.into_inner();

        Ok(Router::new(socket_address, true, self.hub_sender.clone(), incoming_stream, outgoing_route).await)
    }

    /// Connect to a new peer with `retry_attempts` retries and `retry_interval` duration between each attempt
    pub(crate) async fn connect_with_retry(
        &self,
        address: String,
        retry_attempts: u8,
        retry_interval: Duration,
    ) -> Option<Arc<Router>> {
        for counter in 0..retry_attempts {
            match self.connect(address.clone()).await {
                Ok(router) => {
                    debug!("P2P, Client connected, ip & port: {:?}", address);
                    return Some(router);
                }
                Err(err) => {
                    debug!("P2P, Client connect retry #{} failed with error {:?}, ip & port: {:?}", counter, err, address);
                    // Await `retry_interval` time before retrying
                    tokio::time::sleep(retry_interval).await;
                }
            }
        }
        warn!("P2P, Client connection retry #{} - all failed", retry_attempts);
        None
    }

    fn outgoing_network_channel_size() -> usize {
        128
    }

    fn communication_timeout() -> u64 {
        10_000
    }

    fn keep_alive() -> u64 {
        10_000
    }

    fn connect_timeout() -> u64 {
        10_000
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
        // NOTE: No need to explicitly handle the returned router, it will internally be sent to the central Hub
        let _router = Router::new(remote_address, false, self.hub_sender.clone(), incoming_stream, outgoing_route).await;

        // Give tonic a receiver stream (messages sent to it will be forwarded to the network peer)
        Ok(Response::new(Box::pin(ReceiverStream::new(outgoing_receiver).map(Ok)) as Self::MessageStreamStream))
    }
}
