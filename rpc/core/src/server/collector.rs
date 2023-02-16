use crate::notify::{collector::CollectorFrom, connection::ChannelConnection};
use async_channel::{Receiver, Sender};
use consensus_core::notify::Notification as ConsensusNotification;
use kaspa_utils::channel::Channel;

pub(crate) type ConsensusCollector = CollectorFrom<ConsensusNotification, ChannelConnection>;

pub type ConsensusNotificationChannel = Channel<ConsensusNotification>;
pub type ConsensusNotificationSender = Sender<ConsensusNotification>;
pub type ConsensusNotificationReceiver = Receiver<ConsensusNotification>;
