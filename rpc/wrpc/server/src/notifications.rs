use rpc_core::api::ops::RpcApiOps;
use rpc_core::{notify::listener::*, NotificationMessage};
use std::sync::Arc;
use workflow_core::channel::*;
use crate::connection::Connection;
use futures::future::*;
use futures::*;
use std::collections::HashMap;
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
                        match msg {
                            Ok(msg) => {
                                let NotificationMessage { id, payload } = &*msg;
                                if let Some(connection) = listeners.get(id) {
                                    connection.messenger().notify(RpcApiOps::Notification,payload.clone()).await.ok();
                                }
                            },
                            Err(_) => {
                                // TODO
                            }
                        }
                    }

                }
            }
        })
    }
}
