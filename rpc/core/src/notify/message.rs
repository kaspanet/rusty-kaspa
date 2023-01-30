use super::listener::{ListenerID, ListenerSenderSide};
use crate::{Notification, NotificationType};
use std::{fmt::Display, sync::Arc};

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

#[derive(Clone, Debug)]
pub struct NotificationMessage {
    pub id: ListenerID,
    pub payload: Arc<Notification>,
}

impl NotificationMessage {
    pub fn new(id: ListenerID, payload: Arc<Notification>) -> Self {
        NotificationMessage { id, payload }
    }
}

impl From<NotificationMessage> for Arc<Notification> {
    fn from(msg: NotificationMessage) -> Self {
        msg.payload
    }
}

impl Display for NotificationMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.payload)
    }
}
