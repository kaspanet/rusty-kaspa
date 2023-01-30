use crate::NotificationMessage;
use kaspa_utils::channel::Channel;
use kaspa_utils::channel::Sender;
use std::sync::Arc;

pub type NotificationChannel = Channel<Arc<NotificationMessage>>;
pub type NotificationSender = Sender<Arc<NotificationMessage>>;
