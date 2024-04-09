use crate::tasks::Task;
use async_trait::async_trait;
use kaspa_core::warn;
use kaspa_grpc_client::GrpcClient;
use kaspa_utils::triggers::SingleTrigger;
use std::{sync::Arc, time::Duration};
use tokio::{task::JoinHandle, time::sleep};

pub struct NotificationDrainerTask {
    clients: Vec<Arc<GrpcClient>>,
}

impl NotificationDrainerTask {
    pub fn new(clients: Vec<Arc<GrpcClient>>) -> Self {
        Self { clients }
    }

    pub fn build(clients: Vec<Arc<GrpcClient>>) -> Arc<Self> {
        Arc::new(Self::new(clients))
    }
}

#[async_trait]
impl Task for NotificationDrainerTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let clients = self.clients.clone();
        let task = tokio::spawn(async move {
            warn!("Notification drainer task starting...");
            loop {
                tokio::select! {
                    biased;
                    _ = stop_signal.listener.clone() => {
                        break;
                    }
                    _ = sleep(Duration::from_secs(1)) => {}
                }
                clients.iter().for_each(|client| while client.notification_channel_receiver().try_recv().is_ok() {});
            }
            warn!("Notification drainer task exited");
        });
        vec![task]
    }
}
