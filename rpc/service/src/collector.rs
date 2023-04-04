use kaspa_consensus_notify::notification::Notification as ConsensusNotification;
use kaspa_index_core::notification::Notification as IndexNotification;
use kaspa_notify::collector::CollectorFrom;
use kaspa_rpc_core::Notification;

pub(crate) type CollectorFromConsensus = CollectorFrom<ConsensusNotification, Notification>;
pub(crate) type CollectorFromIndex = CollectorFrom<IndexNotification, Notification>;
