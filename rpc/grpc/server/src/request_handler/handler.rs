use super::{
    handler_trait::Handler,
    interface::{DynKaspadMethod, Interface},
};
use crate::{
    connection::{Connection, IncomingRoute},
    connection_handler::ServerContext,
    error::GrpcServerResult,
};
use kaspa_core::debug;
use kaspa_grpc_core::{
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};
use std::fmt::Debug;

pub struct RequestHandler<RpcApiImpl: kaspa_rpc_core::api::rpc::RpcApi + Clone + std::fmt::Debug> {
    rpc_op: KaspadPayloadOps,
    incoming_route: IncomingRoute,
    server_ctx: ServerContext<RpcApiImpl>,
    method: DynKaspadMethod<RpcApiImpl>,
    connection: Connection<RpcApiImpl>,
}

impl<RpcApiImpl: kaspa_rpc_core::api::rpc::RpcApi + std::clone::Clone + Debug> RequestHandler<RpcApiImpl> {
    pub fn new(
        rpc_op: KaspadPayloadOps,
        incoming_route: IncomingRoute,
        server_context: ServerContext<RpcApiImpl>,
        interface: &Interface<RpcApiImpl>,
        connection: Connection<RpcApiImpl>,
    ) -> Self {
        let method = interface.get_method(&rpc_op);
        Self { rpc_op, incoming_route, server_ctx: server_context, method, connection }
    }

    pub async fn handle_request(&self, request: KaspadRequest) -> GrpcServerResult<KaspadResponse> {
        let id = request.id;
        let mut response = self.method.call(self.server_ctx.clone(), self.connection.clone(), request).await?;
        response.id = id;
        Ok(response)
    }
}

#[async_trait::async_trait]
impl<RpcApiImpl: kaspa_rpc_core::api::rpc::RpcApi + std::clone::Clone + std::fmt::Debug> Handler for RequestHandler<RpcApiImpl> {
    async fn start(&mut self) {
        debug!("GRPC, Starting request handler {:?} for client {}", self.rpc_op, self.connection);
        while let Ok(request) = self.incoming_route.recv().await {
            let response = self.handle_request(request).await;
            match response {
                Ok(response) => {
                    if self.connection.enqueue(response).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    debug!("GRPC, Request handling error {} for client {}", e, self.connection);
                }
            }
        }
        debug!("GRPC, Exiting request handler {:?} for client {}", self.rpc_op, self.connection);
    }
}
