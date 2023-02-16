use std::fmt::Debug;
use std::hash::Hash;

pub trait Connection: Clone + Debug + Send + Sync + 'static {
    type Notification;
    type Message: Clone;
    type Encoding: Hash + Clone + Eq + PartialEq + Send;
    type Error: Into<crate::error::Error>;

    fn encoding(&self) -> Self::Encoding;
    fn into_message(notification: &Self::Notification, encoding: &Self::Encoding) -> Self::Message;
    fn send(&self, message: Self::Message) -> Result<(), Self::Error>;
    fn close(&self) -> bool;
    fn is_closed(&self) -> bool;
}
