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
use rpc_core::{
    api::ops::{RpcApiOps, SubscribeCommand},
    notify::{events::EventType, listener::ListenerID, subscriber::SubscriptionManager},
    GetInfoResponse, Notification, NotificationSender, NotificationType, RpcResult,
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
    mpsc::{self, Receiver, Sender},
    oneshot,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Streaming;
use tonic::{codec::CompressionEncoding, transport::Endpoint};

use matcher::*;
mod matcher;

pub const TIMEOUT_DURATION: u64 = 5_000;

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

/// A struct to handle messages flowing to (requests) and from (responses) a protowire server.
/// Incoming responses are associated to pending requests based on their matching operation
/// type and, for some operations like [`ClientApiOps::GetBlock`], on their properties.
///
/// Data flow:
/// ```
/// // KaspadRequest -> send_channel -> recv -> stream -> send -> recv_channel -> KaspadResponse
/// ```
///
/// Execution flow:
/// ```
/// // | call --------------------------------------------------------------------------------->|
/// //                                 | sender_task ----------->| receiver_task -------------->|
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
/// Design/flow:
///
/// Currently call is blocking until receiver_task or timeout_task do solve the pending.
/// So actual concurrency must happen higher in the code.
/// Is there a better way to handle the flow?
///
#[derive(Debug)]
pub(super) struct Resolver {
    handle_stop_notify: bool,

    // Pushing incoming notifications forward
    notify_send: NotificationSender,

    // Sending to server
    request_send: Sender<KaspadRequest>,
    pending_calls: Arc<Mutex<VecDeque<Pending>>>,
    sender_is_running: AtomicBool,
    sender_shutdown: DuplexTrigger,

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
    pub(super) fn new(handle_stop_notify: bool, notify_send: NotificationSender, request_send: Sender<KaspadRequest>) -> Self {
        Self {
            handle_stop_notify,
            notify_send,
            request_send,
            pending_calls: Arc::new(Mutex::new(VecDeque::new())),
            sender_is_running: AtomicBool::new(false),
            sender_shutdown: DuplexTrigger::new(),
            receiver_is_running: AtomicBool::new(false),
            receiver_shutdown: DuplexTrigger::new(),
            timeout_is_running: AtomicBool::new(false),
            timeout_shutdown: DuplexTrigger::new(),
            timeout_duration: TIMEOUT_DURATION,
            timeout_timer_interval: 1_000,
        }
    }

    pub(crate) async fn connect(address: String, notify_send: NotificationSender) -> Result<Arc<Self>> {
        let channel = Endpoint::from_shared(address.clone())?
            .timeout(tokio::time::Duration::from_secs(5))
            .connect_timeout(tokio::time::Duration::from_secs(20))
            .tcp_keepalive(Some(tokio::time::Duration::from_secs(5)))
            .connect()
            .await?;

        let mut client =
            RpcClient::new(channel).send_compressed(CompressionEncoding::Gzip).accept_compressed(CompressionEncoding::Gzip);

        // External channel
        let (request_send, request_recv) = mpsc::channel(16);

        // Force the opening of the stream when connected to a go kaspad server.
        // This is also needed to query server capabilities.
        request_send.send(GetInfoRequestMessage {}.into()).await?;

        // Internal channel
        let (response_send, response_recv) = mpsc::channel(16);

        // Actual KaspadRequest to KaspadResponse stream
        let mut stream: Streaming<KaspadResponse> = client.message_stream(ReceiverStream::new(request_recv)).await?.into_inner();

        // Collect server capabilities as stated in GetInfoResponse
        let mut handle_stop_notify = false;

        match stream.message().await? {
            Some(ref msg) => {
                trace!("GetInfo got response {:?}", msg);
                let response: RpcResult<GetInfoResponse> = msg.try_into();
                if let Ok(response) = response {
                    handle_stop_notify = response.has_notify_command;
                }
            }
            None => {
                return Err(Error::String("gRPC stream was closed by the server".to_string()));
            }
        }

        let resolver = Arc::new(Resolver::new(handle_stop_notify, notify_send, request_send));

        // KaspadRequest timeout cleaner
        resolver.clone().timeout_task();

        // KaspaRequest sender
        resolver.clone().sender_task(stream, response_send);

        // KaspadResponse receiver
        resolver.clone().receiver_task(response_recv);

        Ok(resolver)
    }

    pub(crate) fn handle_stop_notify(&self) -> bool {
        self.handle_stop_notify
    }

    pub(crate) async fn call(&self, op: RpcApiOps, request: impl Into<KaspadRequest>) -> Result<KaspadResponse> {
        let request: KaspadRequest = request.into();
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

    #[allow(unused_must_use)]
    fn timeout_task(self: Arc<Self>) {
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
                        let mut purge = Vec::<usize>::new();
                        let timeout = Duration::from_millis(self.timeout_duration);

                        pending_calls.make_contiguous();
                        let (pending_slice, _) = pending_calls.as_slices();
                        for i in (0..pending_slice.len()).rev() {
                            let pending = pending_calls.get(i).unwrap();
                            if pending.timestamp.elapsed() > timeout {
                                purge.push(i);
                            }
                        }

                        for index in purge.iter() {
                            let pending = pending_calls.remove(*index);
                            if let Some(pending) = pending {

                                trace!("[Resolver] timeout task purged request emmited {:?}", pending.timestamp);

                                // This attribute doesn't seem to work at expression level
                                // So it is duplicated at fn level
                                #[allow(unused_must_use)]
                                pending.sender.send(Err(Error::Timeout));
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

    fn sender_task(self: Arc<Self>, mut stream: Streaming<KaspadResponse>, send: Sender<KaspadResponse>) {
        self.sender_is_running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            loop {
                trace!("[Resolver] sender task loop");

                if send.is_closed() {
                    trace!("[Resolver] sender_task sender is closed");
                    break;
                }

                let shutdown = self.sender_shutdown.request.listener.clone();
                pin_mut!(shutdown);

                tokio::select! {
                    _ = shutdown => { break; }
                    message = stream.message() => {
                        match message {
                            Ok(msg) => {
                                match msg {
                                    Some(response) => {
                                        if let Err(err) = send.send(response).await {
                                            trace!("[Resolver] sender_task sender error: {:?}", err);
                                        }
                                    },
                                    None =>{
                                        trace!("[Resolver] sender_task sender error: no payload");
                                        break;
                                    }
                                }
                            },
                            Err(err) => {
                                trace!("[Resolver] sender_task sender error: {:?}", err);
                            }
                        }
                    }
                }
            }

            trace!("[Resolver] terminating sender task");
            self.sender_is_running.store(false, Ordering::SeqCst);
            self.sender_shutdown.response.trigger.trigger();
        });
    }

    fn receiver_task(self: Arc<Self>, mut recv_channel: Receiver<KaspadResponse>) {
        self.receiver_is_running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            loop {
                trace!("[Resolver] receiver task loop");

                let shutdown = self.receiver_shutdown.request.listener.clone();
                pin_mut!(shutdown);

                tokio::select! {
                    _ = shutdown => { break; }
                    Some(response) = recv_channel.recv() => { self.handle_response(response); }
                }
            }

            trace!("[Resolver] terminating receiver task");
            self.receiver_is_running.store(false, Ordering::SeqCst);
            self.receiver_shutdown.response.trigger.trigger();
        });
    }

    #[allow(unused_must_use)]
    fn handle_response(&self, response: KaspadResponse) {
        if response.is_notification() {
            trace!("[Resolver] handle_response received a notification");
            match Notification::try_from(&response) {
                Ok(notification) => {
                    let event: EventType = (&notification).into();
                    trace!("[Resolver] handle_response received notification: {:?}", event);

                    // Here we ignore any returned error
                    self.notify_send.try_send(Arc::new(notification));
                }
                Err(err) => {
                    trace!("[Resolver] handle_response error converting reponse into notification: {:?}", err);
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
                    pending_calls.make_contiguous();
                    let (pending_slice, _) = pending_calls.as_slices();
                    for i in (0..pending_slice.len()).rev() {
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

                // This attribute doesn't seem to work at expression level
                // So it is duplicated at fn level
                #[allow(unused_must_use)]
                pending.sender.send(Ok(response));
            }
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.stop_timeout().await?;
        self.stop_sender().await?;
        self.stop_receiver().await?;
        Ok(())
    }

    async fn stop_sender(&self) -> Result<()> {
        if self.sender_is_running.load(Ordering::SeqCst) {
            self.sender_shutdown.request.trigger.trigger();
            self.sender_shutdown.response.listener.clone().await;
        }
        Ok(())
    }

    async fn stop_receiver(&self) -> Result<()> {
        if self.receiver_is_running.load(Ordering::SeqCst) {
            self.receiver_shutdown.request.trigger.trigger();
            self.receiver_shutdown.response.listener.clone().await;
        }
        Ok(())
    }

    async fn stop_timeout(&self) -> Result<()> {
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
        // FIXME: Enhance protowire with Subscribe Commands (handle explicit Start)
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
            trace!("[Resolver] stop_notify ignored because not supported by server: {:?}", notification_type);
        }
        Ok(())
    }
}
