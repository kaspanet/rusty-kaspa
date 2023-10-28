use crate::tcp_limiter::grpc::service::LimiterService;
use crate::tcp_limiter::Limit;
use std::sync::Arc;
use tower::Layer;

#[derive(Debug, Clone)]
pub struct LimitLayer {
    limit: Arc<Limit>,
}

impl LimitLayer {
    pub fn new(limit: impl Into<Arc<Limit>>) -> Self {
        Self { limit: limit.into() }
    }
}

impl<S> Layer<S> for LimitLayer {
    type Service = LimiterService<S>;

    fn layer(&self, service: S) -> Self::Service {
        LimiterService::new(service, self.limit.clone())
    }
}
