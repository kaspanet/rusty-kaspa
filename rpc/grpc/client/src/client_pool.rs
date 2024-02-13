use super::GrpcClient;
use async_channel::{SendError, Sender};
use futures_util::Future;
use kaspa_core::trace;
use kaspa_utils::{any::type_name_short, channel::Channel};
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct ClientPool<T> {
    clients: Vec<Arc<GrpcClient>>,
    distribution_channel: Channel<T>,
}

impl<T: Send + 'static> ClientPool<T> {
    pub fn new(clients: Vec<Arc<GrpcClient>>, distribution_channel_capacity: usize) -> Self {
        let distribution_channel = Channel::bounded(distribution_channel_capacity);
        Self { clients, distribution_channel }
    }
    pub fn start<F, R>(&self, client_op: F) -> Vec<JoinHandle<()>>
    where
        F: Fn(Arc<GrpcClient>, T) -> R + Sync + Send + Copy + 'static,
        R: Future<Output = bool> + Send,
    {
        self.clients
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, client)| {
                let rx = self.distribution_channel.receiver();
                tokio::spawn(async move {
                    while let Ok(msg) = rx.recv().await {
                        if client_op(client.clone(), msg).await {
                            rx.close();
                            break;
                        }
                    }
                    client.disconnect().await.unwrap();
                    trace!("Client pool {} task {} exited", type_name_short::<Self>(), index);
                })
            })
            .collect()
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
}
