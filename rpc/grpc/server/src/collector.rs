use super::connection::GrpcConnection;
use kaspa_notify::collector::CollectorFrom;
use kaspa_rpc_core::Notification;

pub type GrpcServiceCollector = CollectorFrom<Notification, Notification, GrpcConnection>;
