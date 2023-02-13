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
use tokio::sync::mpsc::{channel as mpsc_channel, Sender as MpscSender};
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::{Error as TonicError, Server as TonicServer};
use tonic::{Request, Response, Status as TonicStatus, Streaming};

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
    pub(crate) fn serve(&self, serve_address: String) -> Result<OneshotSender<()>, TonicError> {
        info!("P2P, Start Listener, ip & port: {:?}", serve_address);
        let (termination_sender, termination_receiver) = oneshot_channel::<()>();
        let connection_handler = self.clone();
        tokio::spawn(async move {
            debug!("P2P, Listener starting, ip & port: {:?}....", serve_address);
            let proto_server = ProtoP2pServer::new(connection_handler)
                .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
                .send_compressed(tonic::codec::CompressionEncoding::Gzip);

            let serve_result = TonicServer::builder()
                .add_service(proto_server)
                .serve_with_shutdown(serve_address.to_socket_addrs().unwrap().next().unwrap(), termination_receiver.map(drop))
                .await;

            match serve_result {
                Ok(_) => debug!("P2P, Server stopped, ip & port: {:?}", serve_address),
                Err(err) => panic!("P2P, Server stopped with error: {err:?}, ip & port: {serve_address:?}"),
            }
        });
        Ok(termination_sender)
    }

    /// Connect to a new peer
    pub(crate) async fn connect(&self, peer_address: String) -> Result<Arc<Router>, TonicError> {
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
        let incoming_stream = client.message_stream(ReceiverStream::new(outgoing_receiver)).await.unwrap().into_inner();

        Ok(Router::new(self.hub_sender.clone(), incoming_stream, outgoing_route).await)
    }

    /// Connect to a new peer with `retry_attempts` retries and `retry_interval` duration between each attempt
    pub(crate) async fn connect_with_retry(
        &self,
        address: String,
        retry_attempts: u8,
        retry_interval: Duration,
    ) -> Option<Arc<Router>> {
        for counter in 0..retry_attempts {
            if let Ok(router) = self.connect(address.clone()).await {
                debug!("P2P, Client connected, ip & port: {:?}", address);
                return Some(router);
            } else {
                // Asynchronously sleep `retry_interval` time before retrying
                tokio::time::sleep(retry_interval).await;
                if counter % 2 == 0 {
                    debug!("P2P, Client connect retry #{}, ip & port: {:?}", counter, address);
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
        // Build the in/out pipes
        let (outgoing_route, outgoing_receiver) = mpsc_channel(Self::outgoing_network_channel_size());
        let incoming_stream = request.into_inner();

        // Build the router object
        // NOTE: No need to explicitly handle the returned router, it will internally be sent to the central Hub
        let _router = Router::new(self.hub_sender.clone(), incoming_stream, outgoing_route).await;

        // Give tonic a receiver stream (messages sent to it will be forwarded to the network peer)
        Ok(Response::new(Box::pin(ReceiverStream::new(outgoing_receiver).map(Ok)) as Self::MessageStreamStream))
    }
}
