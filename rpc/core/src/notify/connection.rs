use crate::{Notification, NotificationSender};
use std::fmt::Debug;
use std::hash::Hash;

pub trait Connection: Clone + Debug + Send + Sync + 'static {
    type Message: Clone;
    type Encoding: Hash + Clone + Eq + PartialEq + Send;
    type Error: Into<crate::notify::error::Error>;

    fn encoding(&self) -> Self::Encoding;
    fn into_message(notification: &Notification, encoding: &Self::Encoding) -> Self::Message;
    fn send(&self, message: Self::Message) -> Result<(), Self::Error>;
    fn close(&self) -> bool;
    fn is_closed(&self) -> bool;
}

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
    type Message = Notification;
    type Encoding = Unchanged;
    type Error = super::error::Error;

    fn encoding(&self) -> Self::Encoding {
        Unchanged::Clone
    }

    fn into_message(notification: &Notification, _: &Self::Encoding) -> Self::Message {
        notification.clone()
    }

    fn send(&self, message: Self::Message) -> Result<(), Self::Error> {
        match !self.is_closed() {
            true => Ok(self.sender.try_send(message)?),
            false => Err(super::error::Error::ConnectionClosed),
        }
    }

    fn close(&self) -> bool {
        self.sender.close()
    }

    fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}
