use super::process_queue::ProcessQueue;
use itertools::Itertools;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_core::debug;
use kaspa_p2p_lib::{
    make_message,
    pb::{kaspad_message::Payload, InvTransactionsMessage, KaspadMessage},
    Hub,
};
use std::time::{Duration, Instant};

/// Interval between mempool scanning tasks (in seconds)
const SCANNING_TASK_INTERVAL: u64 = 10;
const REBROADCAST_FREQUENCY: u64 = 3;
const BROADCAST_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const MAX_INV_PER_TX_INV_MSG: usize = 131_072;

pub struct TransactionsSpread {
    hub: Hub,
    last_scanning_time: Instant,
    scanning_task_running: bool,
    scanning_job_count: u64,
    transaction_ids: ProcessQueue<TransactionId>,
    last_broadcast_time: Instant,
}

impl TransactionsSpread {
    pub fn new(hub: Hub) -> Self {
        Self {
            hub,
            last_scanning_time: Instant::now(),
            scanning_task_running: false,
            scanning_job_count: 0,
            transaction_ids: ProcessQueue::new(),
            last_broadcast_time: Instant::now(),
        }
    }

    /// Returns true if the time has come for running the task of scanning mempool transactions
    /// and if so, mark the task as running.
    pub fn should_run_mempool_scanning_task(&mut self) -> bool {
        let now = Instant::now();
        if self.scanning_task_running || now < self.last_scanning_time + Duration::from_secs(SCANNING_TASK_INTERVAL) {
            return false;
        }
        let delta = now.checked_duration_since(self.last_scanning_time).expect("verified above");
        // Keep the launching times aligned to exact intervals. Note that `delta=10.1` seconds will result in
        // adding 10 seconds to last scan time, while `delta=11` will result in adding 20 (assuming scanning
        // interval is 10 seconds).
        self.last_scanning_time +=
            Duration::from_secs(((delta.as_secs() + SCANNING_TASK_INTERVAL - 1) / SCANNING_TASK_INTERVAL) * SCANNING_TASK_INTERVAL);

        self.scanning_job_count += 1;
        self.scanning_task_running = true;
        true
    }

    /// Returns true if the time for a rebroadcast of the mempool high priority transactions has come.
    pub fn should_rebroadcast(&self) -> bool {
        self.scanning_job_count % REBROADCAST_FREQUENCY == 0
    }

    pub fn mempool_scanning_job_count(&self) -> u64 {
        self.scanning_job_count
    }

    pub fn mempool_scanning_is_done(&mut self) {
        assert!(self.scanning_task_running, "no stop without a matching start");
        self.scanning_task_running = false;
    }

    /// Add the given transactions IDs to a set of IDs to broadcast. The IDs will be broadcasted to all peers
    /// within transaction Inv messages.
    ///
    /// The broadcast itself may happen only during a subsequent call to this function since it is done at most
    /// every [`BROADCAST_INTERVAL`] milliseconds or when the queue length is larger than the Inv message
    /// capacity.
    ///
    /// _GO-KASPAD: EnqueueTransactionIDsForPropagation_
    pub async fn broadcast_transactions<I: IntoIterator<Item = TransactionId>>(&mut self, transaction_ids: I) {
        self.transaction_ids.enqueue_chunk(transaction_ids);

        let now = Instant::now();
        if now < self.last_broadcast_time + BROADCAST_INTERVAL && self.transaction_ids.len() < MAX_INV_PER_TX_INV_MSG {
            return;
        }

        while !self.transaction_ids.is_empty() {
            let ids = self.transaction_ids.dequeue_chunk(MAX_INV_PER_TX_INV_MSG).map(|x| x.into()).collect_vec();
            debug!("Transaction propagation: broadcasting {} transactions", ids.len());
            let msg = make_message!(Payload::InvTransactions, InvTransactionsMessage { ids });
            self.broadcast(msg).await;
        }

        self.last_broadcast_time = Instant::now();
    }

    async fn broadcast(&self, msg: KaspadMessage) {
        self.hub.broadcast(msg).await
    }
}
