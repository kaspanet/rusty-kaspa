use super::{
    handler::{RequestHandler, SubscriptionHandler},
    handler_trait::Handler,
};
use crate::connection::{Connection, GrpcNotifier, IncomingRoute};
use kaspa_rpc_core::api::{ops::RpcApiOps, rpc::DynRpcService};
use std::sync::Arc;

pub struct HandlerFactory {}

impl HandlerFactory {
    pub fn new_handler(
        rpc_op: RpcApiOps,
        connection: Connection,
        core_service: &DynRpcService,
        notifier: Arc<GrpcNotifier>,
        incoming_route: IncomingRoute,
    ) -> Box<dyn Handler> {
        match rpc_op.is_subscription() {
            false => Box::new(RequestHandler::new(rpc_op, connection, core_service.clone(), incoming_route)),
            true => Box::new(SubscriptionHandler::new(rpc_op, connection.clone(), notifier, connection.listener_id(), incoming_route)),
        }
    }
}
