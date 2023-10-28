use crate::{
    error::{Error, Result},
    resolver::{matcher::Matcher, KaspadResponseReceiver, KaspadResponseSender, Resolver},
};
use kaspa_core::trace;
use kaspa_grpc_core::{
    ops::KaspadPayloadOps,
    protowire::{KaspadRequest, KaspadResponse},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::oneshot;

#[derive(Debug)]
struct Pending {
    timestamp: Instant,
    op: KaspadPayloadOps,
    request: KaspadRequest,
    sender: KaspadResponseSender,
}

impl Pending {
    fn new(op: KaspadPayloadOps, request: KaspadRequest, sender: KaspadResponseSender) -> Self {
        Self { timestamp: Instant::now(), op, request, sender }
    }

    fn is_matching(&self, response: &KaspadResponse, response_op: KaspadPayloadOps) -> bool {
        self.op == response_op && self.request.is_matching(response)
    }
}

#[derive(Debug)]
pub(crate) struct QueueResolver {
    pending_calls: Arc<Mutex<VecDeque<Pending>>>,
}

impl QueueResolver {
    pub(crate) fn new() -> Self {
        Self { pending_calls: Arc::new(Mutex::new(VecDeque::new())) }
    }
}

impl Resolver for QueueResolver {
    fn register_request(&self, op: KaspadPayloadOps, request: &KaspadRequest) -> KaspadResponseReceiver {
        let (sender, receiver) = oneshot::channel::<Result<KaspadResponse>>();
        {
            let pending = Pending::new(op, request.clone(), sender);

            let mut pending_calls = self.pending_calls.lock().unwrap();
            pending_calls.push_back(pending);
            drop(pending_calls);
        }
        receiver
    }

    fn handle_response(&self, response: KaspadResponse) {
        let response_op: KaspadPayloadOps = response.payload.as_ref().unwrap().try_into().expect("response is not a notification");
        trace!("[Resolver] handle_response type: {:?}", response_op);
        let mut pending_calls = self.pending_calls.lock().unwrap();
        let mut pending: Option<Pending> = None;
        if pending_calls.front().is_some() {
            if pending_calls.front().unwrap().is_matching(&response, response_op) {
                pending = pending_calls.pop_front();
            } else {
                let pending_slice = pending_calls.make_contiguous();
                // Iterate the queue front to back, so older pendings first
                for i in 0..pending_slice.len() {
                    if pending_calls.get(i).unwrap().is_matching(&response, response_op) {
                        pending = pending_calls.remove(i);
                        break;
                    }
                }
            }
        }
        drop(pending_calls);
        if let Some(pending) = pending {
            trace!("[Resolver] handle_response matching request found: {:?}", pending.request);
            match pending.sender.send(Ok(response)) {
                Ok(_) => {}
                Err(err) => {
                    trace!("[Resolver] handle_response failed to send the response of a pending: {:?}", err);
                }
            }
        }
    }

    fn remove_expired_requests(&self, timeout: std::time::Duration) {
        let mut pending_calls = self.pending_calls.lock().unwrap();
        let mut index: usize = 0;
        loop {
            if index >= pending_calls.len() {
                break;
            }
            let pending = pending_calls.get(index).unwrap();
            if pending.timestamp.elapsed() > timeout {
                let pending = pending_calls.remove(index).unwrap();
                match pending.sender.send(Err(Error::Timeout)) {
                    Ok(_) => {}
                    Err(err) => {
                        trace!("[Resolver] the timeout monitor failed to send a timeout error: {:?}", err);
                    }
                }
            } else {
                // The call to pending_calls.remove moves whichever end is closer to the
                // removal point. So to prevent skipping items, we only increment index when
                // no removal occurs.
                index += 1;
            }
        }
    }
}
