use super::{
    connection::Connection,
    listener::{ListenerID, ListenerSenderSide},
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
    AddListener(ListenerID, Arc<ListenerSenderSide<T>>),
    RemoveListener(ListenerID),
    Shutdown,
}

#[derive(Clone, Debug)]
pub(crate) enum SubscribeMessage {
    StartEvent(Scope),
    StopEvent(Scope),
    Shutdown,
}
