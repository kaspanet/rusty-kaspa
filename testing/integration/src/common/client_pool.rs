use async_channel::{SendError, Sender};
use futures_util::Future;
use kaspa_core::trace;
use kaspa_grpc_client::GrpcClient;
use kaspa_utils::{any::type_name_short, channel::Channel};
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct ClientPool<T> {
    distribution_channel: Channel<T>,
    pub join_handles: Vec<JoinHandle<()>>,
}

impl<T: Send + 'static> ClientPool<T> {
    pub fn new<F, R>(clients: Vec<Arc<GrpcClient>>, distribution_channel_capacity: usize, client_op: F) -> Self
    where
        F: Fn(Arc<GrpcClient>, T) -> R + Sync + Send + Copy + 'static,
        R: Future<Output = bool> + Send,
    {
        let distribution_channel = Channel::bounded(distribution_channel_capacity);
        let join_handles = clients
            .into_iter()
            .enumerate()
            .map(|(index, client)| {
                let rx = distribution_channel.receiver();
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
            .collect();

        Self { distribution_channel, join_handles }
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
