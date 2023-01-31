use crate::connection::Connection;
use futures::future::*;
use futures::*;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
use rpc_core::{notify::listener::*, NotificationMessage};
use std::collections::HashMap;
use std::sync::Arc;
use workflow_core::channel::*;
use workflow_core::task::*;
use workflow_log::*;

pub type ListenerId = ListenerID;

pub struct NotificationManager {
    pub ingest: Channel<Arc<NotificationMessage>>,
    pub tasks: Vec<Arc<NotificationTask>>,
}

impl NotificationManager {
    pub fn new(tasks: usize) -> Self {
        let ingest = Channel::unbounded();
        let tasks = [0..tasks].iter().map(|_| Arc::new(NotificationTask::new(ingest.clone()))).collect::<Vec<_>>();
        NotificationManager { ingest, tasks }
    }

    pub fn register_notification_listener(&self, id: ListenerId, connection: Connection) {
        self.tasks.iter().for_each(|task| {
            task.ctl
                .try_send(Ctl::Register(id, connection.clone()))
                .unwrap_or_else(|err| log_error!("wRPC::NotificationManager::task.ctl.try_send(): {err}"));
        })
    }

    pub fn unregister_notification_listener(&self, id: ListenerId) {
        self.tasks.iter().for_each(|task| {
            task.ctl
                .try_send(Ctl::Unregister(id))
                .unwrap_or_else(|err| log_error!("wRPC::NotificationManager::task.ctl.try_send(): {err}"));
        })
    }

    pub async fn disconnect(&self, rpc_api: Arc<dyn RpcApi>, connection: Connection) {
        let subscriptions = connection.drain_subscriptions();
        if !subscriptions.is_empty() {
            for id in subscriptions.iter() {
                rpc_api.unregister_listener(*id).await.unwrap_or_else(|err| {
                    log_error!("wRPC::NotificationManager::rpc_api.unregister_listener() unable to unregister listener: `{err}`");
                });
                self.tasks.iter().for_each(|task| {
                    task.ctl.try_send(Ctl::Unregister(*id)).unwrap_or_else(|err| {
                        log_error!(
                            "wRPC::NotificationManager::task.ctl.try_send() unable to unsubscribe on connection close: `{err}`"
                        );
                    });
                });
            }
        }
    }
}

pub enum Ctl {
    Shutdown,
    Register(ListenerId, Connection),
    Unregister(ListenerId),
}

pub struct NotificationTask {
    pub ctl: Channel<Ctl>,
    pub ingest: Channel<Arc<NotificationMessage>>,
    pub completion: Channel<()>,
}

impl NotificationTask {
    pub fn new(ingest: Channel<Arc<NotificationMessage>>) -> Self {
        Self { ctl: Channel::unbounded(), ingest, completion: Channel::oneshot() }
    }

    pub async fn run(self: Arc<Self>) {
        let ctl = self.ctl.receiver.clone();
        let ingest = self.ingest.receiver.clone();
        spawn(async move {
            let mut listeners = HashMap::<ListenerId, Connection>::default();

            loop {
                select! {
                    ctl = ctl.recv().fuse() => {
                        if let Ok(ctl) = ctl {
                            match ctl {
                                Ctl::Register(id,connection) => {
                                    listeners.insert(id, connection);
                                },
                                Ctl::Unregister(id) => {
                                    listeners.remove(&id);
                                },
                                Ctl::Shutdown => {
                                    break;
                                }
                            }
                        }
                    },

                    msg = ingest.recv().fuse() => {
                        if let Ok(msg) = msg {
                            let NotificationMessage { id, payload } = &*msg;
                            if let Some(connection) = listeners.get(id) {
                                let notification_op: RpcApiOps = (&**payload).into();
                                connection.messenger().notify(notification_op,payload.clone()).await.ok();
                            }
                        }
                    }

                }
            }
        })
    }
}
