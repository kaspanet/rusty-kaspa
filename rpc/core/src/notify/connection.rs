use crate::Notification;

pub type ChannelConnection = kaspa_notify::connection::ChannelConnection<Notification>;
pub use kaspa_notify::connection::ChannelType;
