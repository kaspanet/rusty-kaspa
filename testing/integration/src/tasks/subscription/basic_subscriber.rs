use crate::tasks::{subscription::submitter::SubscribeCommand, Task};
use async_channel::Sender;
use async_trait::async_trait;
use kaspa_core::warn;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::scope::Scope;
use kaspa_utils::triggers::SingleTrigger;
use std::{sync::Arc, time::Duration};
use tokio::{sync::oneshot::channel, task::JoinHandle, time::sleep};

pub struct BasicSubscriberTask {
    clients: Vec<Arc<GrpcClient>>,
    subscriptions: Vec<Scope>,
    command_sender: Sender<SubscribeCommand>,
    initial_secs_delay: u64,
}

impl BasicSubscriberTask {
    pub fn new(
        clients: Vec<Arc<GrpcClient>>,
        subscriptions: Vec<Scope>,
        command_sender: Sender<SubscribeCommand>,
        initial_secs_delay: u64,
    ) -> Self {
        Self { clients, subscriptions, command_sender, initial_secs_delay }
    }

    pub fn build(
        clients: Vec<Arc<GrpcClient>>,
        subscriptions: Vec<Scope>,
        command_sender: Sender<SubscribeCommand>,
        initial_secs_delay: u64,
    ) -> Arc<Self> {
        Arc::new(Self::new(clients, subscriptions, command_sender, initial_secs_delay))
    }

    pub fn clients(&self) -> &[Arc<GrpcClient>] {
        &self.clients
    }
}

#[async_trait]
impl Task for BasicSubscriberTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let clients = self.clients.clone();
        let subscriptions = self.subscriptions.clone();
        let sender = self.command_sender.clone();
        let initial_secs_delay = self.initial_secs_delay;
        let task = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = stop_signal.listener.clone() => {
                    return;
                }
                _ = sleep(Duration::from_secs(initial_secs_delay)) => {}
            }
            warn!("Basic subscriber task starting...");
            'outer: for scope in subscriptions {
                let (tx, rx) = channel();
                sender.send(SubscribeCommand::RegisterJob((clients.len(), tx))).await.unwrap();
                let registration = rx.await.unwrap();
                for client in clients.iter().cloned() {
                    if stop_signal.listener.is_triggered() {
                        break 'outer;
                    }
                    sender.send(SubscribeCommand::Start((registration.id, client, scope.clone()))).await.unwrap();
                }
                tokio::select! {
                    biased;
                    _ = stop_signal.listener.clone() => {
                        break;
                    }
                    _ = registration.complete => {}
                }
            }
            warn!("Basic subscriber task exited");
        });
        vec![task]
    }
}
