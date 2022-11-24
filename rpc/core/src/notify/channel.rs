use crate::Notification;
use kaspa_utils::channel::Channel;
use std::sync::Arc;

pub type NotificationChannel = Channel<Arc<Notification>>;
