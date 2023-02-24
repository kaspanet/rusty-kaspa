use crate::Notification;
use consensus_notify::notification::Notification as ConsensusNotification;
use kaspa_index_processor::notification::Notification as IndexNotification;
use kaspa_notify::collector::CollectorFrom;

pub(crate) type CollectorFromConsensus = CollectorFrom<ConsensusNotification, Notification>;
pub(crate) type CollectorFromIndex = CollectorFrom<IndexNotification, Notification>;
