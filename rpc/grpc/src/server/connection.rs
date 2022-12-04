use crate::{protowire::KaspadResponse, server::StatusResult};
use futures::pin_mut;
use kaspa_core::trace;
use kaspa_utils::triggers::DuplexTrigger;
use rpc_core::notify::{
    listener::{ListenerID, ListenerReceiverSide},
    notifier::Notifier,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::mpsc;

pub type GrpcSender = mpsc::Sender<StatusResult<KaspadResponse>>;

pub(crate) struct GrpcConnection {
    address: SocketAddr,
    sender: GrpcSender,
    notify_listener: ListenerReceiverSide,
    collect_shutdown: Arc<DuplexTrigger>,
    collect_is_running: Arc<AtomicBool>,
}

impl GrpcConnection {
    pub(crate) fn new(address: SocketAddr, sender: GrpcSender, notify_listener: ListenerReceiverSide) -> Self {
        Self {
            address,
            sender,
            notify_listener,
            collect_shutdown: Arc::new(DuplexTrigger::new()),
            collect_is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn start(self: Arc<Self>) {
        self.spawn_collecting_task();
    }

    async fn stop(self: Arc<Self>) {
        self.stop_collect().await
    }

    fn spawn_collecting_task(&self) {
        let listener_id = self.notify_listener.id;
        let sender = self.sender.clone();
        let collect_shutdown = self.collect_shutdown.clone();
        let collect_is_running = self.collect_is_running.clone();
        let recv_channel = self.notify_listener.recv_channel.clone();
        collect_is_running.store(true, Ordering::SeqCst);

        tokio::task::spawn(async move {
            trace!("[GrpcConnection] collect_task listener id {0}: start", listener_id);
            loop {
                let shutdown = collect_shutdown.request.listener.clone();
                pin_mut!(shutdown);

                tokio::select! {
                    _ = shutdown => { break; }
                    notification = recv_channel.recv() => {
                        match notification {
                            Ok(notification) => {
                                trace!("sending {} to listener id {}", notification, listener_id);
                                match sender.send(Ok((&*notification).into())).await {
                                    Ok(_) => (),
                                    Err(err) => {

                                        // TODO: we need to decide here if we close connection immediately, or wait for TTL to close it

                                        trace!("[Connection] notification sender error: {:?}", err);
                                    },
                                }
                            },
                            Err(err) => {
                                trace!("[Connection] notification receiver error: {:?}", err);
                            }
                        }
                    }
                }
            }
            collect_is_running.store(false, Ordering::SeqCst);
            collect_shutdown.response.trigger.trigger();
            trace!("[GrpcConnection] collect_task listener id {0}: stop", listener_id);
        });
    }

    async fn stop_collect(&self) {
        if self.collect_is_running.load(Ordering::SeqCst) {
            self.collect_shutdown.request.trigger.trigger();
            self.collect_shutdown.response.listener.clone().await;
        }
    }
}

pub(crate) struct GrpcConnectionManager {
    connections: HashMap<SocketAddr, Arc<GrpcConnection>>,
    notifier: Arc<Notifier>,
}

impl GrpcConnectionManager {
    pub fn new(notifier: Arc<Notifier>) -> Self {
        Self { connections: HashMap::new(), notifier }
    }

    pub(crate) async fn register(&mut self, address: SocketAddr, sender: GrpcSender) -> ListenerID {
        let notify_listener = self.notifier.clone().register_new_listener(None);
        let connection = Arc::new(GrpcConnection::new(address, sender, notify_listener));
        trace!("registering a new gRPC connection from: {0} with listener id {1}", connection.address, connection.notify_listener.id);

        // A pre-existing connection with same address is ignored here
        // TODO: see if some close pattern can be applied to the replaced connection
        self.connections.insert(address, connection.clone());
        connection.clone().start();
        connection.notify_listener.id
    }

    pub(crate) async fn unregister(&mut self, address: SocketAddr) {
        if let Some(connection) = self.connections.remove(&address) {
            trace!("dismiss a gRPC connection from: {}", connection.address);
            //connection.sender.closed().await;
            connection.stop().await;
        }
    }
}
