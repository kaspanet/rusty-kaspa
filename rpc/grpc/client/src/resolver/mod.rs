use super::error::Result;
use aero_grpc_core::{
    ops::AerodPayloadOps,
    protowire::{AerodRequest, AerodResponse},
};
use core::fmt::Debug;
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;

pub(crate) mod id;
pub(crate) mod matcher;
pub(crate) mod queue;

pub(crate) trait Resolver: Send + Sync + Debug {
    fn register_request(&self, op: AerodPayloadOps, request: &AerodRequest) -> AerodResponseReceiver;
    fn handle_response(&self, response: AerodResponse);
    fn remove_expired_requests(&self, timeout: Duration);
}

pub(crate) type DynResolver = Arc<dyn Resolver>;

pub(crate) type AerodResponseSender = oneshot::Sender<Result<AerodResponse>>;
pub(crate) type AerodResponseReceiver = oneshot::Receiver<Result<AerodResponse>>;
