use crate::notify::collector::CollectorFrom;
use async_channel::{Receiver, Sender};
use consensus_core::notify::ConsensusNotification;
use kaspa_utils::channel::Channel;
use std::sync::Arc;

pub(crate) type ConsensusCollector = CollectorFrom<ConsensusNotification>;

pub type ConsensusNotificationChannel = Channel<Arc<ConsensusNotification>>;
pub type ConsensusNotificationSender = Sender<Arc<ConsensusNotification>>;
pub type ConsensusNotificationReceiver = Receiver<Arc<ConsensusNotification>>;
