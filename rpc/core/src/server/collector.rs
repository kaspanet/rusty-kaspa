use async_channel::{Receiver, Sender};
use consensus_core::notify::ConsensusNotification;
use kaspa_utils::channel::Channel;
use std::sync::Arc;

pub use crate::notify::collector::consensus_collector::ConsensusCollector;

pub type ConsensusNotificationChannel = Channel<Arc<ConsensusNotification>>;
pub type ConsensusNotificationSender = Sender<Arc<ConsensusNotification>>;
pub type ConsensusNotificationReceiver = Receiver<Arc<ConsensusNotification>>;
