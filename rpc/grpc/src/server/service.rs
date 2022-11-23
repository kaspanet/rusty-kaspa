use super::connection::{GrpcConnectionManager, GrpcSender};
use crate::protowire::{
    kaspad_request::Payload, rpc_server::Rpc, GetBlockResponseMessage, GetInfoResponseMessage, KaspadRequest, KaspadResponse,
    NotifyBlockAddedResponseMessage,
};
use crate::server::StatusResult;
use futures::Stream;
use rpc_core::notify::channel::NotificationChannel;
use rpc_core::notify::listener::{ListenerID, ListenerReceiverSide, SendingChangedUtxo};
use rpc_core::notify::subscriber::DynSubscriptionManager;
use rpc_core::notify::subscriber::Subscriber;
use rpc_core::RpcResult;
use rpc_core::{
    api::rpc::RpcApi as RpcApiT,
    notify::{collector::RpcCoreCollector, events::EVENT_TYPE_ARRAY, notifier::Notifier},
    server::service::RpcApi,
};
use std::{io::ErrorKind, net::SocketAddr, pin::Pin, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use tonic::{Request, Response};

/// A protowire RPC service.
///
/// Relay requests to a central core service that queries the consensus.
///
/// Registers into a central core service in order to receive consensus notifications and
/// send those forward to the registered clients.
///
///
/// ### Implementation notes
///
/// The service is a listener of the provided core service. The registration happens in the constructor,
/// giving it the lifetime of the overall service.
///
/// As a corollary, the unregistration should occur just before the object is dropped by calling finalize.
///
/// #### Lifetime and usage
///
/// - new -> Self
///     - start
///         - register_connection
///         - unregister_connection
///     - stop
/// - finalize
///
/// _Object is ready for being dropped. Any further usage of it is undefined behaviour._
///
/// #### Further development
///
/// TODO: implement a queue of requests and a pool of workers preparing and sending back the reponses.
pub struct RpcService {
    core_service: Arc<RpcApi>,
    core_channel: NotificationChannel,
    core_listener: Arc<ListenerReceiverSide>,
    connection_manager: Arc<RwLock<GrpcConnectionManager>>,
    notifier: Arc<Notifier>,
}

impl RpcService {
    pub fn new(core_service: Arc<RpcApi>) -> Self {
        // Prepare core objects
        let core_channel = NotificationChannel::default();
        let core_listener = Arc::new(core_service.register_new_listener(Some(core_channel.clone())));

        // Prepare internals
        let collector = Arc::new(RpcCoreCollector::new(core_channel.receiver()));
        let subscription_manager: DynSubscriptionManager = core_service.notifier();
        let subscriber = Subscriber::new(subscription_manager, core_listener.id);
        let notifier = Arc::new(Notifier::new(Some(collector), Some(subscriber), SendingChangedUtxo::FilteredByAddress));
        let connection_manager = Arc::new(RwLock::new(GrpcConnectionManager::new(notifier.clone())));

        Self { core_service, core_channel, core_listener, connection_manager, notifier }
    }

    pub fn start(&self) {
        // Start the internal notifier
        self.notifier.clone().start();
    }

    pub async fn register_connection(&self, address: SocketAddr, sender: GrpcSender) -> ListenerID {
        self.connection_manager.write().await.register(address, sender).await
    }

    pub async fn unregister_connection(&self, address: SocketAddr) {
        self.connection_manager.write().await.unregister(address).await;
    }

    pub async fn stop(&self) -> RpcResult<()> {
        // Unsubscribe from all notification types
        let listener_id = self.core_listener.id;
        for event in EVENT_TYPE_ARRAY.into_iter() {
            self.core_service.stop_notify(listener_id, event.into()).await?;
        }

        // Stop the internal notifier
        self.notifier.clone().stop().await?;

        Ok(())
    }

    pub async fn finalize(&self) -> RpcResult<()> {
        self.core_service.unregister_listener(self.core_listener.id).await?;
        self.core_channel.receiver().close();
        Ok(())
    }
}

#[tonic::async_trait]
impl Rpc for RpcService {
    type MessageStreamStream = Pin<Box<dyn Stream<Item = Result<KaspadResponse, tonic::Status>> + Send + Sync + 'static>>;

    async fn message_stream(
        &self,
        request: Request<tonic::Streaming<KaspadRequest>>,
    ) -> Result<Response<Self::MessageStreamStream>, tonic::Status> {
        let remote_addr = request.remote_addr().ok_or_else(|| {
            tonic::Status::new(tonic::Code::InvalidArgument, "Incoming connection opening request has no remote address".to_string())
        })?;

        println!("MessageStream from {:?}", remote_addr);

        // External sender and reciever
        let (send_channel, mut recv_channel) = mpsc::channel::<StatusResult<KaspadResponse>>(128);
        let listener_id = self.register_connection(remote_addr, send_channel.clone()).await;

        // Internal related sender and reciever
        let (stream_tx, stream_rx) = mpsc::channel::<StatusResult<KaspadResponse>>(10);

        // KaspadResponse forwarder
        let connection_manager = self.connection_manager.clone();
        tokio::spawn(async move {
            while let Some(msg) = recv_channel.recv().await {
                match stream_tx.send(msg).await {
                    Ok(_) => {}
                    Err(_) => {
                        // If sending failed, then remove the connection from connection manager
                        println!("[Remote] stream tx sending error. Remote {:?}", &remote_addr);
                        connection_manager.write().await.unregister(remote_addr).await;
                    }
                }
            }
        });

        // Request handler
        let core_service = self.core_service.clone();
        let connection_manager = self.connection_manager.clone();
        let notifier = self.notifier.clone();
        let mut stream: tonic::Streaming<KaspadRequest> = request.into_inner();
        tokio::spawn(async move {
            loop {
                match stream.message().await {
                    Ok(Some(request)) => {
                        println!("Request is {:?}", request);
                        let response: KaspadResponse = match request.payload {
                            Some(Payload::GetBlockRequest(ref request)) => match request.try_into() {
                                Ok(request) => core_service.get_block(request).await.into(),
                                Err(err) => GetBlockResponseMessage::from(err).into(),
                            },

                            Some(Payload::GetInfoRequest(ref request)) => match request.try_into() {
                                Ok(request) => core_service.get_info(request).await.into(),
                                Err(err) => GetInfoResponseMessage::from(err).into(),
                            },

                            Some(Payload::NotifyBlockAddedRequest(ref request)) => NotifyBlockAddedResponseMessage::from({
                                let request = rpc_core::NotifyBlockAddedRequest::try_from(request).unwrap();
                                notifier.clone().execute_subscribe_command(
                                    listener_id,
                                    rpc_core::NotificationType::BlockAdded,
                                    request.command,
                                )
                            })
                            .into(),

                            // TODO: This must be replaced by actual handling of all request variants
                            _ => GetBlockResponseMessage::from(rpc_core::RpcError::General(
                                "Server-side API Not implemented".to_string(),
                            ))
                            .into(),
                        };

                        match send_channel.send(Ok(response)).await {
                            Ok(_) => {}
                            Err(err) => {
                                println!("tx send error: {:?}", err);
                            }
                        }
                    }
                    Ok(None) => {
                        println!("Request handler stream {0} got Ok(None). Connection terminated by the server", remote_addr);
                        break;
                    }

                    Err(err) => {
                        if let Some(io_err) = match_for_io_error(&err) {
                            if io_err.kind() == ErrorKind::BrokenPipe {
                                // here you can handle special case when client
                                // disconnected in unexpected way
                                eprintln!("\tRequest handler stream {0} error: client disconnected, broken pipe", remote_addr);
                                break;
                            }
                        }

                        match send_channel.send(Err(err)).await {
                            Ok(_) => (),
                            Err(_err) => break, // response was droped
                        }
                    }
                }
            }
            println!("Request handler {0} terminated", remote_addr);
            connection_manager.write().await.unregister(remote_addr).await;
        });

        // Return connection stream

        Ok(Response::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(stream_rx))))
    }
}

fn match_for_io_error(err_status: &tonic::Status) -> Option<&std::io::Error> {
    let mut err: &(dyn std::error::Error + 'static) = err_status;

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
