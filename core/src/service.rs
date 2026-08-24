use crate::core::Core;
use downcast::AnySync;
use std::{sync::Arc, thread::JoinHandle};

pub trait Service: AnySync {
    fn ident(self: Arc<Self>) -> &'static str;
    fn start(self: Arc<Self>, core: Arc<Core>) -> Vec<JoinHandle<()>>;
    fn stop(self: Arc<Self>);
}
