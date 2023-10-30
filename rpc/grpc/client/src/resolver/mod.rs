use super::error::Result;
use core::fmt::Debug;
use kaspa_grpc_core::{
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;

pub(crate) mod id;
pub(crate) mod matcher;
pub(crate) mod queue;

pub(crate) trait Resolver: Send + Sync + Debug {
    fn register_request(&self, op: KaspadPayloadOps, request: &KaspadRequest) -> KaspadResponseReceiver;
    fn handle_response(&self, response: KaspadResponse);
    fn remove_expired_requests(&self, timeout: Duration);
}

pub(crate) type DynResolver = Arc<dyn Resolver>;

pub(crate) type KaspadResponseSender = oneshot::Sender<Result<KaspadResponse>>;
pub(crate) type KaspadResponseReceiver = oneshot::Receiver<Result<KaspadResponse>>;
