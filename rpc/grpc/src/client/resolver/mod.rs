use super::{errors::Error, result::Result};
use crate::protowire::{kaspad_request, rpc_client::RpcClient, GetInfoRequestMessage, KaspadRequest, KaspadResponse};
use async_trait::async_trait;
use futures::{
    future::FutureExt, // for `.fuse()`
    pin_mut,
    select,
};
use kaspa_core::trace;
use kaspa_utils::triggers::DuplexTrigger;
use matcher::*;
use rpc_core::{
    api::ops::{RpcApiOps, SubscribeCommand},
    notify::{events::EventType, listener::ListenerID, subscriber::SubscriptionManager},
    GetInfoResponse, Notification, NotificationMessage, NotificationSender, NotificationType, RpcResult,
};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::{
    mpsc::{self, Sender},
    oneshot,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Streaming;
use tonic::{codec::CompressionEncoding, transport::Endpoint};

mod matcher;

pub type SenderResponse = tokio::sync::oneshot::Sender<Result<KaspadResponse>>;

#[derive(Debug)]
struct Pending {
    timestamp: Instant,
    op: RpcApiOps,
    request: KaspadRequest,
    sender: SenderResponse,
}

impl Pending {
    fn new(op: RpcApiOps, request: KaspadRequest, sender: SenderResponse) -> Self {
        Self { timestamp: Instant::now(), op, request, sender }
    }

    fn is_matching(&self, response: &KaspadResponse, response_op: RpcApiOps) -> bool {
        self.op == response_op && self.request.is_matching(response)
    }
}

pub const CONNECT_TIMEOUT_DURATION: u64 = 20_000;
pub const KEEP_ALIVE_DURATION: u64 = 5_000;
pub const REQUEST_TIMEOUT_DURATION: u64 = 5_000;
pub const TIMEOUT_MONITORING_INTERVAL: u64 = 1_000;

/// A struct to handle messages flowing to (requests) and from (responses) a protowire server.
/// Incoming responses are associated to pending requests based on their matching operation
/// type and, for some operations like [`ClientApiOps::GetBlock`], on their properties.
///
/// Data flow:
/// ```
/// //   KaspadRequest -> request_send -> stream -> KaspadResponse
/// ```
///
/// Execution flow:
/// ```
/// // | call ---------------------------------------------------->|
/// //                                  | response_receiver_task ->|
/// ```
///
///
/// #### Further development
///
/// TODO:
///
/// Carry any subscribe call result up to the initial RpcApiGrpc::start_notify execution.
/// For now, RpcApiGrpc::start_notify only gets a result reflecting the call to
/// Notifier::try_send_dispatch. This is not complete.
///
/// Investigate a possible bottleneck in handle_response with the processing of pendings.
/// If this is the case, some concurrent alternative should be considered.
///
/// Design/flow:
///
/// Currently call is blocking until response_receiver_task or timeout_task do solve the pending.
/// So actual concurrency must happen higher in the code.
/// Is there a better way to handle the flow?
///
#[derive(Debug)]
pub(super) struct Resolver {
    // temporary hack to override the handle_stop_notify flag
    override_handle_stop_notify : bool,

    handle_stop_notify: bool,
    _handle_message_id: bool,

    // Pushing incoming notifications forward
    notify_send: NotificationSender,

    // Sending to server
    request_send: Sender<KaspadRequest>,
    pending_calls: Arc<Mutex<VecDeque<Pending>>>,

    // Receiving from server
    receiver_is_running: AtomicBool,
    receiver_shutdown: DuplexTrigger,

    // Pending timeout cleaning task
    timeout_is_running: AtomicBool,
    timeout_shutdown: DuplexTrigger,
    timeout_timer_interval: u64,
    timeout_duration: u64,
}

impl Resolver {
    pub(super) fn new(
        override_handle_stop_notify: bool,
        handle_stop_notify: bool,
        handle_message_id: bool,
        notify_send: NotificationSender,
        request_send: Sender<KaspadRequest>,
    ) -> Self {
        Self {
            override_handle_stop_notify,
            handle_stop_notify,
            _handle_message_id: handle_message_id,
            notify_send,
            request_send,
            pending_calls: Arc::new(Mutex::new(VecDeque::new())),
            receiver_is_running: AtomicBool::new(false),
            receiver_shutdown: DuplexTrigger::new(),
            timeout_is_running: AtomicBool::new(false),
            timeout_shutdown: DuplexTrigger::new(),
            timeout_duration: REQUEST_TIMEOUT_DURATION,
            timeout_timer_interval: TIMEOUT_MONITORING_INTERVAL,
        }
    }

    // TODO - remove the override (discuss how to handle this in relation to the golang client)
    pub(crate) async fn connect(override_handle_stop_notify: bool, address: String, notify_send: NotificationSender) -> Result<Arc<Self>> {
        let channel = Endpoint::from_shared(address.clone())?
            .timeout(tokio::time::Duration::from_millis(REQUEST_TIMEOUT_DURATION))
            .connect_timeout(tokio::time::Duration::from_millis(CONNECT_TIMEOUT_DURATION))
            .tcp_keepalive(Some(tokio::time::Duration::from_millis(KEEP_ALIVE_DURATION)))
            .connect()
            .await?;

        let mut client =
            RpcClient::new(channel).send_compressed(CompressionEncoding::Gzip).accept_compressed(CompressionEncoding::Gzip);

        // External channel
        let (request_send, request_recv) = mpsc::channel(16);

        // Force the opening of the stream when connected to a go kaspad server.
        // This is also needed for querying server capabilities.
        request_send.send(GetInfoRequestMessage {}.into()).await?;

        // Actual KaspadRequest to KaspadResponse stream
        let mut stream: Streaming<KaspadResponse> = client.message_stream(ReceiverStream::new(request_recv)).await?.into_inner();

        // Collect server capabilities as stated in GetInfoResponse
        let mut handle_stop_notify = false;
        let mut handle_message_id = false;
        match stream.message().await? {
            Some(ref msg) => {
                trace!("GetInfo got response {:?}", msg);
                let response: RpcResult<GetInfoResponse> = msg.try_into();
                if let Ok(response) = response {
                    handle_stop_notify = response.has_notify_command;
                    handle_message_id = response.has_message_id;
                }
            }
            None => {
                return Err(Error::String("gRPC stream was closed by the server".to_string()));
            }
        }

        // create the resolver
        let resolver = Arc::new(Resolver::new(override_handle_stop_notify, handle_stop_notify, handle_message_id, notify_send, request_send));

        // Start the request timeout cleaner
        resolver.clone().spawn_request_timeout_monitor();

        // Start the response receiving task
        resolver.clone().spawn_response_receiver_task(stream);

        Ok(resolver)
    }

    pub(crate) fn handle_stop_notify(&self) -> bool {
        // TODO - remove this
        if self.override_handle_stop_notify {
            true
        } else {
            self.handle_stop_notify
        }
    }

    pub(crate) async fn call(&self, op: RpcApiOps, request: impl Into<KaspadRequest>) -> Result<KaspadResponse> {
        let id = u64::from_le_bytes(rand::random::<[u8; 8]>());
        let mut request: KaspadRequest = request.into();
        request.id = id;

        trace!("resolver call: {:?}", request);
        if request.payload.is_some() {
            let (sender, receiver) = oneshot::channel::<Result<KaspadResponse>>();

            {
                let pending = Pending::new(op, request.clone(), sender);

                let mut pending_calls = self.pending_calls.lock().unwrap();
                pending_calls.push_back(pending);
                drop(pending_calls);
            }

            self.request_send.send(request).await.map_err(|_| Error::ChannelRecvError)?;

            receiver.await?
        } else {
            Err(Error::MissingRequestPayload)
        }
    }

    /// Launch a task that periodically checks pending requests and deletes those that have
    /// waited longer than a predefined delay.
    fn spawn_request_timeout_monitor(self: Arc<Self>) {
        self.timeout_is_running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            let shutdown = self.timeout_shutdown.request.listener.clone().fuse();
            pin_mut!(shutdown);

            loop {
                let timeout_timer_interval = Duration::from_millis(self.timeout_timer_interval);
                let delay = tokio::time::sleep(timeout_timer_interval).fuse();
                pin_mut!(delay);

                select! {
                    _ = shutdown => { break; },
                    _ = delay => {
                        trace!("[Resolver] running timeout task");
                        let mut pending_calls = self.pending_calls.lock().unwrap();
                        let timeout = Duration::from_millis(self.timeout_duration);

                        let mut index: usize = 0;
                        loop {
                            if index >= pending_calls.len() {
                                break;
                            }
                            let pending = pending_calls.get(index).unwrap();
                            if pending.timestamp.elapsed() > timeout {
                                let pending = pending_calls.remove(index).unwrap();
                                match pending.sender.send(Err(Error::Timeout)) {
                                    Ok(_) => {},
                                    Err(err) => {
                                        trace!("[Resolver] the timeout monitor failed to send a timeout error: {:?}", err);
                                    },
                                }
                            } else {
                                // The call to pending_calls.remove moves whichever end is closer to the
                                // removal point. So to prevent skipping items, we only increment index when
                                // no removal occurs.
                                index += 1;
                            }
                        }
                    },
                }
            }

            trace!("[Resolver] terminating timeout task");
            self.timeout_is_running.store(false, Ordering::SeqCst);
            self.timeout_shutdown.response.trigger.trigger();
        });
    }

    /// Launch a task receiving and handling response messages sent by the server.
    fn spawn_response_receiver_task(self: Arc<Self>, mut stream: Streaming<KaspadResponse>) {
        self.receiver_is_running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            loop {
                trace!("[Resolver] response receiver loop");

                let shutdown = self.receiver_shutdown.request.listener.clone();
                pin_mut!(shutdown);

                tokio::select! {
                    _ = shutdown => { break; }
                    message = stream.message() => {
                        match message {
                            Ok(msg) => {
                                match msg {
                                    Some(response) => {
                                        self.handle_response(response);
                                    },
                                    None =>{
                                        trace!("[Resolver] the incoming stream of the response receiver is closed");

                                        // This event makes the whole object unable to work anymore.
                                        // This should be reported to the owner of this Resolver.
                                        //
                                        // Some automatic reconnection mechanism could also be investigated.
                                        break;
                                    }
                                }
                            },
                            Err(err) => {
                                trace!("[Resolver] the response receiver gets an error from the server: {:?}", err);
                            }
                        }
                    }
                }
            }

            trace!("[Resolver] terminating response receiver");
            self.receiver_is_running.store(false, Ordering::SeqCst);
            self.receiver_shutdown.response.trigger.trigger();
        });
    }

    fn handle_response(&self, response: KaspadResponse) {
        if response.is_notification() {
            trace!("[Resolver] handle_response received a notification");
            match Notification::try_from(&response) {
                Ok(notification) => {
                    let event: EventType = (&notification).into();
                    trace!("[Resolver] handle_response received notification: {:?}", event);

                    // Here we ignore any returned error
                    // FIXME - NotificationMessage id is currently initialized to 0!
                    match self.notify_send.try_send(Arc::new(NotificationMessage::new(0, Arc::new(notification)))) {
                        Ok(_) => {}
                        Err(err) => {
                            trace!("[Resolver] error while trying to send a notification to the notifier: {:?}", err);
                        }
                    }
                }
                Err(err) => {
                    trace!("[Resolver] handle_response error converting response into notification: {:?}", err);
                }
            }
        } else if response.payload.is_some() {
            let response_op: RpcApiOps = response.payload.as_ref().unwrap().into();
            trace!("[Resolver] handle_response type: {:?}", response_op);
            let mut pending_calls = self.pending_calls.lock().unwrap();
            let mut pending: Option<Pending> = None;
            if pending_calls.front().is_some() {
                if pending_calls.front().unwrap().is_matching(&response, response_op.clone()) {
                    pending = pending_calls.pop_front();
                } else {
                    let pending_slice = pending_calls.make_contiguous();
                    // Iterate the queue front to back, so older pendings first
                    for i in 0..pending_slice.len() {
                        if pending_calls.get(i).unwrap().is_matching(&response, response_op.clone()) {
                            pending = pending_calls.remove(i);
                            break;
                        }
                    }
                }
            }
            drop(pending_calls);
            if let Some(pending) = pending {
                trace!("[Resolver] handle_response matching request found: {:?}", pending.request);
                match pending.sender.send(Ok(response)) {
                    Ok(_) => {}
                    Err(err) => {
                        trace!("[Resolver] handle_response failed to send the response of a pending: {:?}", err);
                    }
                }
            }
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.stop_timeout_monitor().await?;
        self.stop_response_receiver_task().await?;
        Ok(())
    }

    async fn stop_response_receiver_task(&self) -> Result<()> {
        if self.receiver_is_running.load(Ordering::SeqCst) {
            self.receiver_shutdown.request.trigger.trigger();
            self.receiver_shutdown.response.listener.clone().await;
        }
        Ok(())
    }

    async fn stop_timeout_monitor(&self) -> Result<()> {
        if self.timeout_is_running.load(Ordering::SeqCst) {
            self.timeout_shutdown.request.trigger.trigger();
            self.timeout_shutdown.response.listener.clone().await;
        }
        Ok(())
    }
}

#[async_trait]
impl SubscriptionManager for Resolver {
    async fn start_notify(self: Arc<Self>, _: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        trace!("[Resolver] start_notify: {:?}", notification_type);
        let request = kaspad_request::Payload::from_notification_type(&notification_type, SubscribeCommand::Start);
        self.clone().call((&request).into(), request).await?;
        Ok(())
    }

    async fn stop_notify(self: Arc<Self>, _: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        if self.handle_stop_notify {
            trace!("[Resolver] stop_notify: {:?}", notification_type);
            let request = kaspad_request::Payload::from_notification_type(&notification_type, SubscribeCommand::Stop);
            self.clone().call((&request).into(), request).await?;
        } else {
            trace!("[Resolver] stop_notify ignored because not supported by the server: {:?}", notification_type);
        }
        Ok(())
    }
}
