use super::GrpcClient;
use async_channel::{SendError, Sender};
use futures_util::{future::join_all, Future};
use kaspa_core::trace;
use kaspa_utils::{any::type_name_short, channel::Channel};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct ClientPool<T> {
    distribution_channel: Channel<T>,
    join_handles: Mutex<Vec<JoinHandle<()>>>,
}

impl<T: Send + 'static> ClientPool<T> {
    pub fn new<F, R>(clients: Vec<Arc<GrpcClient>>, distribution_channel_capacity: usize, client_op: F) -> Self
    where
        F: Fn(Arc<GrpcClient>, T) -> R + Sync + Send + Copy + 'static,
        R: Future<Output = bool> + Send,
    {
        let distribution_channel = Channel::bounded(distribution_channel_capacity);
        let join_handles = Mutex::new(
            clients
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
                .collect(),
        );

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

    pub fn join_handles(&self) -> Vec<JoinHandle<()>> {
        self.join_handles.lock().drain(..).collect()
    }

    pub async fn join(&self) {
        join_all(self.join_handles()).await;
    }
}
