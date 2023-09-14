use super::process_queue::ProcessQueue;
use itertools::Itertools;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_core::debug;
use kaspa_p2p_lib::{
    common::ProtocolError,
    make_message,
    pb::{kaspad_message::Payload, InvTransactionsMessage, KaspadMessage},
    Hub,
};
use std::time::{Duration, Instant};

const CLEANING_TASK_INTERVAL: Duration = Duration::from_secs(10);
const REBROADCAST_FREQUENCY: u64 = 3;
const BROADCAST_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const MAX_INV_PER_TX_INV_MSG: usize = 131_072;

pub struct TransactionsSpread {
    hub: Hub,
    last_cleaning_time: Instant,
    cleaning_task_running: bool,
    cleaning_count: u64,
    transaction_ids: ProcessQueue<TransactionId>,
    last_broadcast_time: Instant,
}

impl TransactionsSpread {
    pub fn new(hub: Hub) -> Self {
        Self {
            hub,
            last_cleaning_time: Instant::now(),
            cleaning_task_running: false,
            cleaning_count: 0,
            transaction_ids: ProcessQueue::new(),
            last_broadcast_time: Instant::now(),
        }
    }

    /// Returns true if the time has come for running the task cleaning mempool transactions
    /// and if so, mark the task as running.
    pub fn should_run_cleaning_task(&mut self) -> bool {
        if self.cleaning_task_running || Instant::now() < self.last_cleaning_time + CLEANING_TASK_INTERVAL {
            return false;
        }
        // Keep the launching times aligned to exact intervals
        let call_time = Instant::now();
        while self.last_cleaning_time + CLEANING_TASK_INTERVAL < call_time {
            self.last_cleaning_time += CLEANING_TASK_INTERVAL;
        }
        self.cleaning_count += 1;
        self.cleaning_task_running = true;
        true
    }

    /// Returns true if the time for a rebroadcast of the mempool high priority transactions has come.
    pub fn should_rebroadcast(&self) -> bool {
        self.cleaning_count % REBROADCAST_FREQUENCY == 0
    }

    pub fn cleaning_count(&self) -> u64 {
        self.cleaning_count
    }

    pub fn cleaning_is_done(&mut self) {
        assert!(self.cleaning_task_running, "no stop without a matching start");
        self.cleaning_task_running = false;
    }

    /// Add the given transactions IDs to a set of IDs to broadcast. The IDs will be broadcasted to all peers
    /// within transaction Inv messages.
    ///
    /// The broadcast itself may happen only during a subsequent call to this function since it is done at most
    /// every [`BROADCAST_INTERVAL`] milliseconds or when the queue length is larger than the Inv message
    /// capacity.
    ///
    /// _GO-KASPAD: EnqueueTransactionIDsForPropagation_
    pub async fn broadcast_transactions<I: IntoIterator<Item = TransactionId>>(
        &mut self,
        transaction_ids: I,
    ) -> Result<(), ProtocolError> {
        self.transaction_ids.enqueue_chunk(transaction_ids);

        let now = Instant::now();
        if now < self.last_broadcast_time + BROADCAST_INTERVAL && self.transaction_ids.len() < MAX_INV_PER_TX_INV_MSG {
            return Ok(());
        }

        while !self.transaction_ids.is_empty() {
            let ids = self.transaction_ids.dequeue_chunk(MAX_INV_PER_TX_INV_MSG).map(|x| x.into()).collect_vec();
            debug!("Transaction propagation: broadcasting {} transactions", ids.len());
            let msg = make_message!(Payload::InvTransactions, InvTransactionsMessage { ids });
            self.broadcast(msg).await;
        }

        self.last_broadcast_time = Instant::now();
        Ok(())
    }

    async fn broadcast(&self, msg: KaspadMessage) {
        self.hub.broadcast(msg).await
    }
}
