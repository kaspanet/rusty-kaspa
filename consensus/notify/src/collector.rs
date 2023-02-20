use consensus_core::notify::notification::Notification;
use kaspa_notify::collector::CollectorFrom;

pub type ConsensusCollector = CollectorFrom<Notification, Notification>;
