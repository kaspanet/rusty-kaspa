use super::method::{DropFn, Method, MethodTrait, RoutingPolicy};
use crate::{
    connection::Connection,
    connection_handler::ServerContext,
    error::{GrpcServerError, GrpcServerResult},
};
use aero_grpc_core::{
    ops::AerodPayloadOps,
    protowire::{AerodRequest, AerodResponse},
};
use std::fmt::Debug;
use std::{collections::HashMap, sync::Arc};

pub type AerodMethod = Method<ServerContext, Connection, AerodRequest, AerodResponse>;
pub type DynAerodMethod = Arc<dyn MethodTrait<ServerContext, Connection, AerodRequest, AerodResponse>>;
pub type AerodDropFn = DropFn<AerodRequest, AerodResponse>;
pub type AerodRoutingPolicy = RoutingPolicy<AerodRequest, AerodResponse>;

/// An interface providing methods implementations and a fallback "not implemented" method
/// actually returning a message with a "not implemented" error.
///
/// The interface can provide a method clone for every [`AerodPayloadOps`] variant for later
/// processing of related requests.
///
/// It is also possible to directly let the interface itself process a request by invoking
/// the `call()` method.
pub struct Interface {
    server_ctx: ServerContext,
    methods: HashMap<AerodPayloadOps, DynAerodMethod>,
    method_not_implemented: DynAerodMethod,
}

impl Interface {
    pub fn new(server_ctx: ServerContext) -> Self {
        let method_not_implemented = Arc::new(Method::new(|_, _, aerod_request: AerodRequest| {
            Box::pin(async move {
                match aerod_request.payload {
                    Some(ref request) => Ok(AerodResponse {
                        id: aerod_request.id,
                        payload: Some(AerodPayloadOps::from(request).to_error_response(GrpcServerError::MethodNotImplemented.into())),
                    }),
                    None => Err(GrpcServerError::InvalidRequestPayload),
                }
            })
        }));
        Self { server_ctx, methods: Default::default(), method_not_implemented }
    }

    pub fn method(&mut self, op: AerodPayloadOps, method: AerodMethod) {
        let method: DynAerodMethod = Arc::new(method);
        if self.methods.insert(op, method).is_some() {
            panic!("RPC method {op:?} is declared multiple times")
        }
    }

    pub fn replace_method(&mut self, op: AerodPayloadOps, method: AerodMethod) {
        let method: DynAerodMethod = Arc::new(method);
        let _ = self.methods.insert(op, method);
    }

    pub fn set_method_properties(&mut self, op: AerodPayloadOps, tasks: usize, queue_size: usize, routing_policy: AerodRoutingPolicy) {
        self.methods.entry(op).and_modify(|x| {
            let method: Method<ServerContext, Connection, AerodRequest, AerodResponse> =
                Method::with_properties(x.method_fn(), tasks, queue_size, routing_policy);
            let method: Arc<dyn MethodTrait<ServerContext, Connection, AerodRequest, AerodResponse>> = Arc::new(method);
            *x = method;
        });
    }

    pub async fn call(&self, op: &AerodPayloadOps, connection: Connection, request: AerodRequest) -> GrpcServerResult<AerodResponse> {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).call(self.server_ctx.clone(), connection, request).await
    }

    pub fn get_method(&self, op: &AerodPayloadOps) -> DynAerodMethod {
        self.methods.get(op).unwrap_or(&self.method_not_implemented).clone()
    }
}

impl Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interface").finish()
    }
}
