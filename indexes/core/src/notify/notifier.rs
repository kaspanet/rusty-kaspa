use crate::notify::notification::Notification as IndexNotification;
use kaspa_notify::{connection::ChannelConnection, notifier::Notifier};

pub type IndexNotifier = Notifier<IndexNotification, ChannelConnection<IndexNotification>>;
