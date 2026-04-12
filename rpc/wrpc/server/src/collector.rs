use keryx_notify::{collector::CollectorFrom, converter::ConverterFrom};
use keryx_rpc_core::Notification;

pub type WrpcServiceConverter = ConverterFrom<Notification, Notification>;
pub type WrpcServiceCollector = CollectorFrom<WrpcServiceConverter>;
