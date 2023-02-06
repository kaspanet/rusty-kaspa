use super::listener::{ListenerID, ListenerSenderSide};
use crate::{Notification, NotificationType};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) enum DispatchMessage {
    Send(Arc<Notification>),
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
