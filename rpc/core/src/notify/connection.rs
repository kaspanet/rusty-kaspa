use crate::Notification;

pub type ChannelConnection = aero_notify::connection::ChannelConnection<Notification>;
pub use aero_notify::connection::ChannelType;
