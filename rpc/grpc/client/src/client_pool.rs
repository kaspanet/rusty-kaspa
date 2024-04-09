use super::GrpcClient;
use async_channel::{SendError, Sender};
use futures_util::Future;
use itertools::Itertools;
use kaspa_core::trace;
use kaspa_utils::{any::type_name_short, channel::Channel, triggers::SingleTrigger};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::task::JoinHandle;

pub struct ClientPool<T> {
    clients: Vec<Arc<GrpcClient>>,
    distribution_channel: Channel<T>,
    running_tasks: Arc<AtomicUsize>,
    started: SingleTrigger,
    shutdown: SingleTrigger,
}

impl<T: Send + 'static> ClientPool<T> {
    pub fn new(clients: Vec<Arc<GrpcClient>>, distribution_channel_capacity: usize) -> Self {
        let distribution_channel = Channel::bounded(distribution_channel_capacity);
        let running_tasks = Arc::new(AtomicUsize::new(0));
        let started = SingleTrigger::new();
        let shutdown = SingleTrigger::new();
        Self { clients, distribution_channel, running_tasks, started, shutdown }
    }

    pub fn start<F, R>(&self, client_op: F) -> Vec<JoinHandle<()>>
    where
        F: Fn(Arc<GrpcClient>, T) -> R + Sync + Send + Copy + 'static,
        R: Future<Output = bool> + Send,
    {
        let tasks = self
            .clients
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, client)| {
                let running_tasks = self.running_tasks.clone();
                let started_listener = self.started_listener();
                let shutdown_trigger = self.shutdown.trigger.clone();
                let rx = self.distribution_channel.receiver();
                tokio::spawn(async move {
                    let _ = running_tasks.fetch_add(1, Ordering::SeqCst);
                    started_listener.await;
                    while let Ok(msg) = rx.recv().await {
                        if client_op(client.clone(), msg).await {
                            rx.close();
                            break;
                        }
                    }
                    client.disconnect().await.unwrap();
                    trace!("Client pool {} task {} exited", type_name_short::<Self>(), index);
                    if running_tasks.fetch_sub(1, Ordering::SeqCst) == 1 {
                        shutdown_trigger.trigger();
                    }
                })
            })
            .collect_vec();
        self.started.trigger.trigger();
        if tasks.is_empty() {
            self.shutdown.trigger.trigger();
        }
        tasks
    }

    pub fn clients(&self) -> &[Arc<GrpcClient>] {
        &self.clients
    }

    pub async fn send_via_available_client(&self, msg: T) -> Result<(), SendError<T>> {
        self.distribution_channel.send(msg).await
    }

    pub fn sender(&self) -> Sender<T> {
        self.distribution_channel.sender()
    }

    pub fn close(&self) {
        self.distribution_channel.close()
    }

    pub fn shutdown_listener(&self) -> triggered::Listener {
        self.shutdown.listener.clone()
    }

    pub fn started_listener(&self) -> triggered::Listener {
        self.started.listener.clone()
    }
}
