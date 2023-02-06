use super::connection::GrpcConnection;
use kaspa_rpc_core::{notify::collector::CollectorFrom, Notification};

pub type GrpcServiceCollector = CollectorFrom<Notification, GrpcConnection>;
