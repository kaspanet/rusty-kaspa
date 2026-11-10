use aero_notify::notifier::Notify;
use aero_rpc_core::Notification;
use async_channel::Sender;

#[derive(Debug)]
pub struct ChannelNotify {
    sender: Sender<Notification>,
}

impl ChannelNotify {
    pub fn new(sender: Sender<Notification>) -> Self {
        Self { sender }
    }
}

impl Notify<Notification> for ChannelNotify {
    fn notify(&self, notification: Notification) -> aero_notify::error::Result<()> {
        self.sender.try_send(notification)?;
        Ok(())
    }
}
