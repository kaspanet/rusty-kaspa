use crate::Notification;
use async_channel::{Receiver, Sender};
use consensus_core::notify::notification::Notification as ConsensusNotification;
use event_processor::notify::Notification as EventProcessorNotification;
use kaspa_notify::collector::CollectorFrom;
use kaspa_utils::channel::Channel;

pub(crate) type CollectorFromConsensus = CollectorFrom<ConsensusNotification, Notification>;

pub(crate) type CollectorFromEventProcessor = CollectorFrom<EventProcessorNotification, Notification>;

pub type EventProcessorNotificationChannel = Channel<EventProcessorNotification>;
pub type EventProcessorNotificationSender = Sender<EventProcessorNotification>;
pub type EventProcessorNotificationReceiver = Receiver<EventProcessorNotification>;
