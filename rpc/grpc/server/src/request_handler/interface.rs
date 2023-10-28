use super::method::{Method, MethodTrait};
use crate::{
    connection::Connection,
    connection_handler::ServerContext,
    error::{GrpcServerError, GrpcServerResult},
};
use kaspa_grpc_core::{
    convert::kaspad::build_error_response,
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};
use std::fmt::Debug;
use std::{collections::HashMap, sync::Arc};

pub type DynMethod = Arc<dyn MethodTrait<ServerContext, Connection, KaspadRequest, KaspadResponse>>;

pub struct Interface {
    server_ctx: ServerContext,
    methods: HashMap<KaspadPayloadOps, DynMethod>,
    method_not_implemented: DynMethod,
}

impl Interface {
    pub fn new(server_ctx: ServerContext) -> Self {
        let method_not_implemented = Arc::new(Method::new(|_, _, kaspad_request: KaspadRequest| {
            Box::pin(async move {
                match kaspad_request.payload {
                    Some(ref request) => Ok(KaspadResponse {
                        id: kaspad_request.id,
                        payload: Some(build_error_response(request, GrpcServerError::MethodNotImplemented.into())),
                    }),
                    None => Err(GrpcServerError::InvalidRequestPayload),
                }
            })
        }));
        Self { server_ctx, methods: Default::default(), method_not_implemented }
    }

    pub fn method(&mut self, op: KaspadPayloadOps, method: Method<ServerContext, Connection, KaspadRequest, KaspadResponse>) {
        let method: Arc<dyn MethodTrait<ServerContext, Connection, KaspadRequest, KaspadResponse>> = Arc::new(method);
        if self.methods.insert(op, method).is_some() {
            panic!("RPC method {op:?} is declared multiple times")
        }
    }

    pub async fn call(
        &self,
        op: &KaspadPayloadOps,
        connection: Connection,
        request: KaspadRequest,
    ) -> GrpcServerResult<KaspadResponse> {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).call(self.server_ctx.clone(), connection, request).await
    }

    pub fn get_method(&self, op: &KaspadPayloadOps) -> DynMethod {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).clone()
    }
}

impl Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interface").finish()
    }
}