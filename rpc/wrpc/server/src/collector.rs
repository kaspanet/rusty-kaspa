use kaspa_notify::collector::CollectorFrom;
use kaspa_rpc_core::Notification;

pub type WrpcServiceCollector = CollectorFrom<Notification, Notification>;
