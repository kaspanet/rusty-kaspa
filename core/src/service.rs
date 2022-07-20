use crate::core::Core;
use intertrait::CastFromSync;
use std::{sync::Arc, thread::JoinHandle};

pub trait Service: CastFromSync {
    fn ident(self: Arc<Self>) -> String;
    fn start(self: Arc<Self>, core: Arc<Core>) -> Vec<JoinHandle<()>>;
    fn stop(self: Arc<Self>);
}
