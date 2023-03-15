use crate::Notification;
use kaspa_notify::collector::CollectorFrom;

/// A rpc_core notification collector providing a simple pass-through.
/// No conversion occurs since both source and target data are of
/// type [`Notification`].
pub type RpcCoreCollector = CollectorFrom<Notification, Notification>;
