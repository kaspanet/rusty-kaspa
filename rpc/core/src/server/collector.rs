use crate::notify::collector::CollectorFrom;
use async_channel::{Receiver, Sender};
use event_processor::notify::Notification as EventNotification;
use kaspa_utils::channel::Channel;

pub(crate) type EventNotificationCollector = CollectorFrom<EventNotification>;

pub type EventNotificationChannel = Channel<EventNotification>;
pub type EventNotificationSender = Sender<EventNotification>;
pub type EventNotificationReceiver = Receiver<EventNotification>;
