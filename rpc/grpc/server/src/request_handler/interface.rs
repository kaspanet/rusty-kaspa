use super::method::{DropFn, Method, MethodTrait, RoutingPolicy};
use crate::{
    connection::Connection,
    connection_handler::ServerContext,
    error::{GrpcServerError, GrpcServerResult},
};
use kaspa_grpc_core::{
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};
use std::fmt::Debug;
use std::{collections::HashMap, sync::Arc};

pub type KaspadMethod = Method<ServerContext, Connection, KaspadRequest, KaspadResponse>;
pub type DynKaspadMethod = Arc<dyn MethodTrait<ServerContext, Connection, KaspadRequest, KaspadResponse>>;
pub type KaspadDropFn = DropFn<KaspadRequest, KaspadResponse>;
pub type KaspadRoutingPolicy = RoutingPolicy<KaspadRequest, KaspadResponse>;

/// An interface providing methods implementations and a fallback "not implemented" method
/// actually returning a message with a "not implemented" error.
///
/// The interface can provide a method clone for every [`KaspadPayloadOps`] variant for later
/// processing of related requests.
///
/// It is also possible to directly let the interface itself process a request by invoking
/// the `call()` method.
pub struct Interface {
    server_ctx: ServerContext,
    methods: HashMap<KaspadPayloadOps, DynKaspadMethod>,
    method_not_implemented: DynKaspadMethod,
}

impl Interface {
    pub fn new(server_ctx: ServerContext) -> Self {
        let method_not_implemented = Arc::new(Method::new(|_, _, kaspad_request: KaspadRequest| {
            Box::pin(async move {
                match kaspad_request.payload {
                    Some(ref request) => Ok(KaspadResponse {
                        id: kaspad_request.id,
                        payload: Some(KaspadPayloadOps::from(request).to_error_response(GrpcServerError::MethodNotImplemented.into())),
                    }),
                    None => Err(GrpcServerError::InvalidRequestPayload),
                }
            })
        }));
        Self { server_ctx, methods: Default::default(), method_not_implemented }
    }

    pub fn method(&mut self, op: KaspadPayloadOps, method: KaspadMethod) {
        let method: DynKaspadMethod = Arc::new(method);
        if self.methods.insert(op, method).is_some() {
            panic!("RPC method {op:?} is declared multiple times")
        }
    }

    pub fn replace_method(&mut self, op: KaspadPayloadOps, method: KaspadMethod) {
        let method: DynKaspadMethod = Arc::new(method);
        let _ = self.methods.insert(op, method);
    }

    pub fn set_method_properties(
        &mut self,
        op: KaspadPayloadOps,
        tasks: usize,
        queue_size: usize,
        routing_policy: KaspadRoutingPolicy,
    ) {
        self.methods.entry(op).and_modify(|x| {
            let method: Method<ServerContext, Connection, KaspadRequest, KaspadResponse> =
                Method::with_properties(x.method_fn(), tasks, queue_size, routing_policy);
            let method: Arc<dyn MethodTrait<ServerContext, Connection, KaspadRequest, KaspadResponse>> = Arc::new(method);
            *x = method;
        });
    }

    pub async fn call(
        &self,
        op: &KaspadPayloadOps,
        connection: Connection,
        request: KaspadRequest,
    ) -> GrpcServerResult<KaspadResponse> {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).call(self.server_ctx.clone(), connection, request).await
    }

    pub fn get_method(&self, op: &KaspadPayloadOps) -> DynKaspadMethod {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).clone()
    }
}

impl Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interface").finish()
    }
}
