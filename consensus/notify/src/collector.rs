use crate::notification::Notification;
use kaspa_notify::collector::CollectorFrom;

pub type ConsensusCollector = CollectorFrom<Notification, Notification>;
