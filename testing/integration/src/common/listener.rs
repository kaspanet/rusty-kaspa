use async_channel::Receiver;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::{connection::ChannelType, events::EventType, listener::ListenerId, scope::Scope, subscription::Command};
use kaspa_rpc_core::{api::rpc::RpcApi, notify::connection::ChannelConnection, Notification, RpcResult};

/// An event type bound notification listener
#[derive(Clone)]
pub struct Listener {
    client: GrpcClient,
    id: ListenerId,
    event: EventType,
    pub receiver: Receiver<Notification>,
}

impl Listener {
    pub async fn subscribe(client: GrpcClient, scope: Scope) -> RpcResult<Listener> {
        let (sender, receiver) = async_channel::unbounded();
        let connection = ChannelConnection::new("client listener", sender, ChannelType::Closable);
        let id = client.register_new_listener(connection);
        let event = scope.event_type();
        client.start_notify(id, scope).await?;
        let listener = Listener { client, id, event, receiver };
        Ok(listener)
    }

    pub async fn execute_subscribe_command(&self, scope: Scope, command: Command) -> RpcResult<()> {
        assert_eq!(self.event, (&scope).into());
        self.client.execute_subscribe_command(self.id, scope, command).await
    }

    pub fn drain(&self) {
        while !self.receiver.is_empty() {
            let _ = self.receiver.try_recv().unwrap();
        }
    }
}
