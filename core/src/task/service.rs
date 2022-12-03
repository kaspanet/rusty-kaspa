use intertrait::CastFromSync;
use tokio::task::JoinHandle;

use std::sync::Arc;

pub trait AsyncService: CastFromSync {
    fn ident(self: Arc<Self>) -> &'static str;
    fn start(self: Arc<Self>) -> JoinHandle<()>;
    fn signal_exit(self: Arc<Self>);
    fn stop(self: Arc<Self>) -> JoinHandle<()>;
}
