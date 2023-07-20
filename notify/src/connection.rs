use crate::error::Error;
use crate::notification::Notification;
use async_channel::Sender;
use std::fmt::Debug;
use std::hash::Hash;

pub trait Connection: Clone + Debug + Send + Sync + 'static {
    type Notification;
    type Message: Clone;
    type Encoding: Hash + Clone + Eq + PartialEq + Send;
    type Error: Into<Error>;

    fn encoding(&self) -> Self::Encoding;
    fn into_message(notification: &Self::Notification, encoding: &Self::Encoding) -> Self::Message;
    fn send(&self, message: Self::Message) -> Result<(), Self::Error>;
    fn close(&self) -> bool;
    fn is_closed(&self) -> bool;
}

#[derive(Clone, Debug)]
pub enum ChannelType {
    Closable,
    Persistent,
}

#[derive(Clone, Debug)]
pub struct ChannelConnection<N>
where
    N: Notification,
{
    sender: Sender<N>,
    channel_type: ChannelType,
}

impl<N> ChannelConnection<N>
where
    N: Notification,
{
    pub fn new(sender: Sender<N>, channel_type: ChannelType) -> Self {
        Self { sender, channel_type }
    }

    /// Close the connection, ignoring the channel type
    pub fn force_close(&self) -> bool {
        self.sender.close()
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Default)]
pub enum Unchanged {
    #[default]
    Clone = 0,
}

impl<N> Connection for ChannelConnection<N>
where
    N: Notification,
{
    type Notification = N;
    type Message = N;
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
        match self.channel_type {
            ChannelType::Closable => self.sender.close(),
            ChannelType::Persistent => false,
        }
    }

    fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}
