use futures_util::future::BoxFuture;
use intertrait::CastFromSync;

use std::sync::Arc;

pub type AsyncServiceFuture = BoxFuture<'static, ()>;

pub trait AsyncService: CastFromSync {
    fn ident(self: Arc<Self>) -> &'static str;
    fn start(self: Arc<Self>) -> AsyncServiceFuture;
    fn signal_exit(self: Arc<Self>);
    fn stop(self: Arc<Self>) -> AsyncServiceFuture;
}
