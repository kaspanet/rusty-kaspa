use super::{daemon::Daemon, listener::Listener};
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::{events::EventType, scope::Scope, subscription::Command};
use kaspa_rpc_core::RpcResult;
use std::{
    collections::{hash_map::Entry, HashMap},
    ops::Deref,
};

/// A multi-listener gRPC client with event type dedicated listeners
pub struct ListeningClient {
    client: GrpcClient,
    listeners: HashMap<EventType, Listener>,
}

impl ListeningClient {
    pub async fn connect(kaspad: &Daemon) -> Self {
        let client = kaspad.new_multi_listener_client().await;
        client.start(None).await;
        let listeners = Default::default();
        ListeningClient { client, listeners }
    }

    pub async fn start_notify(&mut self, scope: Scope) -> RpcResult<()> {
        let event = scope.event_type();
        match self.listeners.entry(event) {
            Entry::Occupied(e) => e.get().execute_subscribe_command(scope, Command::Start).await,
            Entry::Vacant(e) => {
                e.insert(Listener::subscribe(self.client.clone(), scope).await?);
                Ok(())
            }
        }
    }

    #[allow(dead_code)]
    pub async fn stop_notify(&mut self, scope: Scope) -> RpcResult<()> {
        let event = scope.event_type();
        match self.listeners.entry(event) {
            Entry::Occupied(e) => e.get().execute_subscribe_command(scope, Command::Stop).await,
            Entry::Vacant(_) => Ok(()),
        }
    }

    pub fn listener(&self, event: EventType) -> Option<Listener> {
        self.listeners.get(&event).cloned()
    }

    pub fn block_added_listener(&self) -> Option<Listener> {
        self.listener(EventType::BlockAdded)
    }

    pub fn utxos_changed_listener(&self) -> Option<Listener> {
        self.listener(EventType::UtxosChanged)
    }

    pub fn virtual_daa_score_changed_listener(&self) -> Option<Listener> {
        self.listener(EventType::VirtualDaaScoreChanged)
    }
}

impl Deref for ListeningClient {
    type Target = GrpcClient;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}
