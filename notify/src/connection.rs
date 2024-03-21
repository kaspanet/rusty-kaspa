use crate::error::Error;
use crate::notification::Notification;
use async_channel::Sender;
use std::fmt::{Debug, Display};
use std::hash::Hash;

#[async_trait::async_trait]
pub trait Connection: Clone + Display + Debug + Send + Sync + 'static {
    type Notification;
    type Message: Clone + Send + Sync;
    type Encoding: Hash + Clone + Eq + PartialEq + Send + Sync;
    type Error: Into<Error>;

    fn encoding(&self) -> Self::Encoding;
    fn into_message(notification: &Self::Notification, encoding: &Self::Encoding) -> Self::Message;
    async fn send(&self, message: Self::Message) -> Result<(), Self::Error>;
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
    name: &'static str,
    sender: Sender<N>,
    channel_type: ChannelType,
}

impl<N> ChannelConnection<N>
where
    N: Notification,
{
    pub fn new(name: &'static str, sender: Sender<N>, channel_type: ChannelType) -> Self {
        Self { name, sender, channel_type }
    }

    /// Close the connection, ignoring the channel type
    pub fn force_close(&self) -> bool {
        self.sender.close()
    }
}

impl<N> Display for ChannelConnection<N>
where
    N: Notification,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Default)]
pub enum Unchanged {
    #[default]
    Clone = 0,
}

#[async_trait::async_trait]
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

    async fn send(&self, message: Self::Message) -> Result<(), Self::Error> {
        match !self.is_closed() {
            true => Ok(self.sender.send(message).await?),
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
