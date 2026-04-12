use async_channel::Sender;
use keryx_notify::notifier::Notify;
use keryx_rpc_core::Notification;

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
    fn notify(&self, notification: Notification) -> keryx_notify::error::Result<()> {
        self.sender.try_send(notification)?;
        Ok(())
    }
}
