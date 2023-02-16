use crate::{Notification, NotificationSender};
use kaspa_notify::{connection::Connection, error::Error};

#[derive(Clone, Debug)]
pub struct ChannelConnection {
    sender: NotificationSender,
}

impl ChannelConnection {
    pub fn new(sender: NotificationSender) -> Self {
        Self { sender }
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Default)]
pub enum Unchanged {
    #[default]
    Clone = 0,
}

impl Connection for ChannelConnection {
    type Notification = Notification;
    type Message = Notification;
    type Encoding = Unchanged;
    type Error = Error;

    fn encoding(&self) -> Self::Encoding {
        Unchanged::Clone
    }

    fn into_message(notification: &Self::Notification, _: &Self::Encoding) -> Self::Message {
        notification.clone()
    }

    fn send(&self, message: Self::Message) -> Result<(), Self::Error> {
        match !self.is_closed() {
            true => Ok(self.sender.try_send(message)?),
            false => Err(Error::ConnectionClosed),
        }
    }

    fn close(&self) -> bool {
        self.sender.close()
    }

    fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}
