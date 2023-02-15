use super::{
    connection::Connection,
    listener::{ListenerID, ListenerSenderSide},
};
use crate::{Notification, NotificationType};
use std::sync::Arc;

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] //TODO: solution: use targeted Arcs on large variants in `rpc-core::api::notifications::Notification` to reduce size
pub(crate) enum DispatchMessage<T> 
where
    T: Connection,
{
    Send(Notification),
    AddListener(ListenerID, Arc<ListenerSenderSide>),
    RemoveListener(ListenerID),
    Shutdown,
}

#[derive(Clone, Debug)]
pub(crate) enum SubscribeMessage {
    StartEvent(NotificationType),
    StopEvent(NotificationType),
    Shutdown,
}
