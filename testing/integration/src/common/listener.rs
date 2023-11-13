use async_channel::Receiver;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::{connection::ChannelType, listener::ListenerId, scope::Scope};
use kaspa_rpc_core::{api::rpc::RpcApi, notify::connection::ChannelConnection, Notification};

pub struct Listener {
    _id: ListenerId,
    pub receiver: Receiver<Notification>,
}

impl Listener {
    pub async fn subscribe(client: &GrpcClient, scope: Scope) -> Listener {
        let (sender, receiver) = async_channel::unbounded();
        let connection = ChannelConnection::new(sender, ChannelType::Closable);
        let _id = client.register_new_listener(connection);
        client.start_notify(_id, scope).await.unwrap();
        Listener { _id, receiver }
    }

    pub fn drain(&self) {
        while !self.receiver.is_empty() {
            let _ = self.receiver.try_recv().unwrap();
        }
    }
}
