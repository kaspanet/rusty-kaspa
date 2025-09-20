use crate::error::GrpcServerResult;
use async_trait::async_trait;
use futures::Future;
use std::{pin::Pin, sync::Arc};

pub enum RoutingPolicy<Request, Response> {
    Enqueue,
    DropIfFull(DropFn<Request, Response>),
}

impl<Request, Response> Clone for RoutingPolicy<Request, Response> {
    fn clone(&self) -> Self {
        match self {
            Self::Enqueue => Self::Enqueue,
            Self::DropIfFull(drop_fn) => Self::DropIfFull(drop_fn.clone()),
        }
    }
}

#[async_trait]
pub trait MethodTrait<ServerContext, ConnectionContext, Request, Response>: Send + Sync + 'static {
    async fn call(&self, server_ctx: ServerContext, connection_ctx: ConnectionContext, request: Request)
        -> GrpcServerResult<Response>;

    fn method_fn(&self) -> MethodFn<ServerContext, ConnectionContext, Request, Response>;
    fn tasks(&self) -> usize;
    fn queue_size(&self) -> usize;
    fn routing_policy(&self) -> RoutingPolicy<Request, Response>;
}

/// RPC method function type
pub type MethodFn<ServerContext, ConnectionContext, Request, Response> =
    Arc<Box<dyn Send + Sync + Fn(ServerContext, ConnectionContext, Request) -> MethodFnReturn<Response> + 'static>>;

/// RPC method function return type
pub type MethodFnReturn<T> = Pin<Box<dyn Send + 'static + Future<Output = GrpcServerResult<T>>>>;

/// RPC drop function type
pub type DropFn<Request, Response> = Arc<Box<dyn Send + Sync + Fn(&Request) -> GrpcServerResult<Response>>>;

/// RPC method wrapper. Contains the method closure function.
pub struct Method<ServerContext, ConnectionContext, Request, Response>
where
    ServerContext: Send + Sync + 'static,
    ConnectionContext: Send + Sync + 'static,
    Request: Send + Sync + 'static,
    Response: Send + Sync + 'static,
{
    /// Function called when executing the method
    method_fn: MethodFn<ServerContext, ConnectionContext, Request, Response>,

    /// Number of connection concurrent request handlers
    tasks: usize,

    /// Size of the request queue
    queue_size: usize,

    /// Policy applied when the routing channel is full
    routing_policy: RoutingPolicy<Request, Response>,
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
        Method {
            method_fn: Arc::new(Box::new(method_fn)),
            tasks: 1,
            queue_size: Self::default_queue_size(),
            routing_policy: RoutingPolicy::Enqueue,
        }
    }

    pub fn with_properties(
        method_fn: MethodFn<ServerContext, ConnectionContext, Request, Response>,
        tasks: usize,
        queue_size: usize,
        routing_policy: RoutingPolicy<Request, Response>,
    ) -> Method<ServerContext, ConnectionContext, Request, Response> {
        Method { method_fn, tasks, queue_size, routing_policy }
    }

    pub fn default_queue_size() -> usize {
        256
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
        (self.method_fn)(server_ctx, connection_ctx, request).await
    }

    fn method_fn(&self) -> MethodFn<ServerContext, ConnectionContext, Request, Response> {
        self.method_fn.clone()
    }

    fn tasks(&self) -> usize {
        self.tasks
    }

    fn queue_size(&self) -> usize {
        self.queue_size
    }

    fn routing_policy(&self) -> RoutingPolicy<Request, Response> {
        self.routing_policy.clone()
    }
}
