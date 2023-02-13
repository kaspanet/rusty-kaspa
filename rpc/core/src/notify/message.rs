use super::{
    connection::Connection,
    listener::{ListenerId, ListenerSenderSide},
    scope::Scope,
};
use crate::Notification;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) enum DispatchMessage<T>
where
    T: Connection,
{
    Send(Arc<Notification>),
    AddListener(ListenerId, Arc<ListenerSenderSide<T>>),
    RemoveListener(ListenerId),
    Shutdown,
}

#[derive(Clone, Debug)]
pub(crate) enum SubscribeMessage {
    StartEvent(Scope),
    StopEvent(Scope),
    Shutdown,
}
