use async_channel::Sender;
use kaspa_notify::notifier::Notify;
use kaspa_rpc_core::Notification;

#[derive(Debug)]
pub struct ChannelNotify {
    pub sender: Sender<Notification>,
}

impl Notify<Notification> for ChannelNotify {
    fn notify(&self, notification: Notification) -> kaspa_notify::error::Result<()> {
        self.sender.try_send(notification)?;
        Ok(())
    }
}
