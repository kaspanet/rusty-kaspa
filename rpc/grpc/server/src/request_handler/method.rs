use crate::error::GrpcServerResult;
use async_trait::async_trait;
use futures::Future;
use std::{pin::Pin, sync::Arc};

#[async_trait]
pub trait MethodTrait<ServerContext, ConnectionContext, Request, Response>: Send + Sync + 'static {
    async fn call(&self, server_ctx: ServerContext, connection_ctx: ConnectionContext, request: Request)
        -> GrpcServerResult<Response>;
}

/// RPC method function type
pub type MethodFn<ServerContext, ConnectionContext, Request, Response> =
    Arc<Box<dyn Send + Sync + Fn(ServerContext, ConnectionContext, Request) -> MethodFnReturn<Response> + 'static>>;

/// RPC method function return type
pub type MethodFnReturn<T> = Pin<Box<(dyn Send + 'static + Future<Output = GrpcServerResult<T>>)>>;

/// RPC method wrapper. Contains the method closure function.
pub struct Method<ServerContext, ConnectionContext, Request, Response>
where
    ServerContext: Send + Sync + 'static,
    ConnectionContext: Send + Sync + 'static,
    Request: Send + Sync + 'static,
    Response: Send + Sync + 'static,
{
    method: MethodFn<ServerContext, ConnectionContext, Request, Response>,
}

impl<ServerContext, ConnectionContext, Request, Response> Method<ServerContext, ConnectionContext, Request, Response>
where
    ServerContext: Send + Sync + 'static,
    ConnectionContext: Send + Sync + 'static,
    Request: Send + Sync + 'static,
    Response: Send + Sync + 'static,
{
    pub fn new<FN>(method_fn: FN) -> Method<ServerContext, ConnectionContext, Request, Response>
    where
        FN: Send + Sync + Fn(ServerContext, ConnectionContext, Request) -> MethodFnReturn<Response> + 'static,
    {
        Method { method: Arc::new(Box::new(method_fn)) }
    }
}

#[async_trait]
impl<ServerContext, ConnectionContext, Request, Response> MethodTrait<ServerContext, ConnectionContext, Request, Response>
    for Method<ServerContext, ConnectionContext, Request, Response>
where
    ServerContext: Clone + Send + Sync + 'static,
    ConnectionContext: Send + Sync + 'static,
    Request: Send + Sync + 'static,
    Response: Send + Sync + 'static,
{
    async fn call(
        &self,
        server_ctx: ServerContext,
        connection_ctx: ConnectionContext,
        request: Request,
    ) -> GrpcServerResult<Response> {
        (self.method)(server_ctx, connection_ctx, request).await
    }
}
