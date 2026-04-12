use crate::Notification;

pub type ChannelConnection = keryx_notify::connection::ChannelConnection<Notification>;
pub use keryx_notify::connection::ChannelType;
