use kaspa_notify::{collector::CollectorFrom, converter::ConverterFrom};
use kaspa_rpc_core::Notification;

pub type WrpcServiceConverter = ConverterFrom<Notification, Notification>;
pub type WrpcServiceCollector = CollectorFrom<WrpcServiceConverter>;
