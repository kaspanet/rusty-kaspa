use crate::tcp_limiter::Limit;
use std::{
    pin::Pin,
    sync::{atomic::Ordering, Arc},
    task::{Context, Poll},
};
use tonic::{Code, Status};
use tower::Service;

#[derive(Debug, Clone)]
pub struct LimiterService<S> {
    inner: S,
    limit: Arc<Limit>,
}

impl<S> LimiterService<S> {
    pub fn new(inner: S, limit: impl Into<Arc<Limit>>) -> LimiterService<S> {
        Self { inner, limit: limit.into() }
    }
}

type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

impl<S, Req> Service<Req> for LimiterService<S>
where
    Req: Send + 'static,
    S: Service<Req, Response = http::Response<tonic::body::BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        let limit = self.limit.clone();

        Box::pin(async move {
            let current = limit.fetch_add(1, Ordering::SeqCst);

            if current >= limit.max {
                limit.fetch_sub(1, Ordering::Release);

                let err_response =
                    Status::new(Code::PermissionDenied, "The gRPC service has reached full capacity and accepts no new connection")
                        .to_http();
                return Ok(err_response);
            }

            let result = inner.call(req).await;
            limit.fetch_sub(1, Ordering::Release);
            result
        })
    }
}
