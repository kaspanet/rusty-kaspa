use crate::{Notification, NotificationSender};
use std::fmt::Debug;
use std::{hash::Hash, sync::Arc};

pub trait Connection: Clone + Debug + Send + Sync + 'static {
    type Message: Clone;
    type Variant: Hash + Clone + Eq + PartialEq + Send;
    type Error: Into<crate::notify::error::Error>;

    fn variant(&self) -> Self::Variant;
    fn into_message(notification: &Arc<Notification>, variant: &Self::Variant) -> Self::Message;
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
pub enum Invariant {
    #[default]
    Default = 0,
}

impl Connection for ChannelConnection {
    type Message = Arc<Notification>;
    type Variant = Invariant;
    type Error = super::error::Error;

    fn variant(&self) -> Self::Variant {
        Invariant::Default
    }

    fn into_message(notification: &Arc<Notification>, _: &Self::Variant) -> Self::Message {
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
