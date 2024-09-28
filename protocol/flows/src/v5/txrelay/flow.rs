use crate::{
    flow_context::{FlowContext, RequestScope},
    flow_trait::Flow,
    flowcontext::transactions::MAX_INV_PER_TX_INV_MSG,
};
use kaspa_consensus_core::tx::{Transaction, TransactionId};
use kaspa_consensusmanager::ConsensusProxy;
use kaspa_core::{time::unix_now, warn};
use kaspa_mining::{
    errors::MiningManagerError,
    mempool::{
        errors::RuleError,
        tx::{Orphan, Priority, RbfPolicy},
    },
    model::tx_query::TransactionQuery,
    P2pTxCountSample,
};
use kaspa_p2p_lib::{
    common::{ProtocolError, DEFAULT_TIMEOUT},
    dequeue, make_message,
    pb::{kaspad_message::Payload, RequestTransactionsMessage, TransactionNotFoundMessage},
    IncomingRoute, Router,
};
use std::sync::Arc;
use tokio::time::timeout;

pub(crate) const MAX_TPS_THRESHOLD: u64 = 3000;

enum Response {
    Transaction(Transaction),
    NotFound(TransactionId),
}

impl Response {
    fn transaction_id(&self) -> TransactionId {
        match self {
            Response::Transaction(tx) => tx.id(),
            Response::NotFound(id) => id.to_owned(),
        }
    }
}

/// Flow listening to InvTransactions messages, requests their corresponding transactions if they
/// are missing, adds them to the mempool and propagates them to the rest of the network.
pub struct RelayTransactionsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    /// A route specific for invs messages
    invs_route: IncomingRoute,
    /// A route for other messages such as Transaction and TransactionNotFound
    msg_route: IncomingRoute,

    /// Track the number of spam txs coming from this peer
    spam_counter: u64,
}

/// Holds the state information for whether we will throttle tx relay or not
struct ThrottlingState {
    should_throttle: bool,
    last_checked_time: u64,
    curr_snapshot: P2pTxCountSample,
}

#[async_trait::async_trait]
impl Flow for RelayTransactionsFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RelayTransactionsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, invs_route: IncomingRoute, msg_route: IncomingRoute) -> Self {
        Self { ctx, router, invs_route, msg_route, spam_counter: 0 }
    }

    pub fn invs_channel_size() -> usize {
        // TODO: reevaluate when the node is fully functional and later when the network tx rate increases
        // Note: in go-kaspad we have 10,000 for this channel combined with tx channel.
        4096
    }

    pub fn txs_channel_size() -> usize {
        // Incoming tx flow capacity must correlate with the max number of invs per tx inv
        // message, since this effectively becomes the upper-bound on number of tx requests
        MAX_INV_PER_TX_INV_MSG
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        // trace!("Starting relay transactions flow with {}", self.router.identity());
        let mut throttling_state = ThrottlingState {
            should_throttle: false,
            last_checked_time: unix_now(),
            curr_snapshot: self.ctx.mining_manager().clone().p2p_tx_count_sample(),
        };

        loop {
            let now = unix_now();
            if now > 10000 + throttling_state.last_checked_time {
                let next_snapshot = self.ctx.mining_manager().clone().p2p_tx_count_sample();
                check_tx_throttling(&mut throttling_state, next_snapshot);
                throttling_state.last_checked_time = now;
            }

            // Loop over incoming block inv messages
            let inv: Vec<TransactionId> = dequeue!(self.invs_route, Payload::InvTransactions)?.try_into()?;
            // trace!("Receive an inv message from {} with {} transaction ids", self.router.identity(), inv.len());

            if inv.len() > MAX_INV_PER_TX_INV_MSG {
                return Err(ProtocolError::Other("Number of invs in tx inv message is over the limit"));
            }

            let session = self.ctx.consensus().unguarded_session();

            // Transaction relay is disabled if the node is out of sync and thus not mining
            if !session.async_is_nearly_synced().await {
                continue;
            }

            let requests = self.request_transactions(inv, throttling_state.should_throttle, &throttling_state.curr_snapshot).await?;
            self.receive_transactions(session, requests, throttling_state.should_throttle).await?;
        }
    }

    async fn request_transactions(
        &self,
        transaction_ids: Vec<TransactionId>,
        should_throttle: bool,
        curr_snapshot: &P2pTxCountSample,
    ) -> Result<Vec<RequestScope<TransactionId>>, ProtocolError> {
        // Build a vector with the transaction ids unknown in the mempool and not already requested
        // by another peer
        let transaction_ids = self.ctx.mining_manager().clone().unknown_transactions(transaction_ids).await;
        let mut requests = Vec::new();
        let snapshot_delta = curr_snapshot - &self.ctx.mining_manager().clone().p2p_tx_count_sample();

        // To reduce the P2P TPS to below the threshold, we need to request up to a max of
        // whatever the balances overage. If MAX_TPS_THRESHOLD is 3000 and the current TPS is 4000,
        // then we can only request up to 2000 (MAX - (4000 - 3000)) to average out into the threshold.
        let curr_p2p_tps = 1000 * snapshot_delta.low_priority_tx_counts / (snapshot_delta.elapsed_time.as_millis().max(1) as u64);
        let overage = if should_throttle && curr_p2p_tps > MAX_TPS_THRESHOLD { curr_p2p_tps - MAX_TPS_THRESHOLD } else { 0 };

        let limit = MAX_TPS_THRESHOLD.saturating_sub(overage);

        for transaction_id in transaction_ids {
            if let Some(req) = self.ctx.try_adding_transaction_request(transaction_id) {
                requests.push(req);
            }

            if should_throttle && requests.len() >= limit as usize {
                break;
            }
        }

        // Request the transactions
        if !requests.is_empty() {
            // trace!("Send a request to {} with {} transaction ids", self.router.identity(), requests.len());
            self.router
                .enqueue(make_message!(
                    Payload::RequestTransactions,
                    RequestTransactionsMessage { ids: requests.iter().map(|x| x.req.into()).collect() }
                ))
                .await?;
        }

        Ok(requests)
    }

    /// Returns the next Transaction or TransactionNotFound message in msg_route,
    /// returning only one of the message types at a time.
    async fn read_response(&mut self) -> Result<Response, ProtocolError> {
        match timeout(DEFAULT_TIMEOUT, self.msg_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::Transaction(payload)) => Ok(Response::Transaction(payload.try_into()?)),
                        Some(Payload::TransactionNotFound(payload)) => Ok(Response::NotFound(payload.try_into()?)),
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(Payload::Transaction | Payload::TransactionNotFound),
                            msg.payload.as_ref().map(|v| v.into()),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => {
                // One reason this may happen is the invs_route being full and preventing
                // the router from routing other incoming messages
                Err(ProtocolError::Timeout(DEFAULT_TIMEOUT))
            }
        }
    }

    async fn receive_transactions(
        &mut self,
        consensus: ConsensusProxy,
        requests: Vec<RequestScope<TransactionId>>,
        should_throttle: bool,
    ) -> Result<(), ProtocolError> {
        let mut transactions: Vec<Transaction> = Vec::with_capacity(requests.len());
        for request in requests {
            let response = self.read_response().await?;
            let transaction_id = response.transaction_id();
            if transaction_id != request.req {
                return Err(ProtocolError::OtherOwned(format!(
                    "requested transaction id {} but got transaction {}",
                    request.req, transaction_id
                )));
            }
            if let Response::Transaction(transaction) = response {
                transactions.push(transaction);
            }
        }
        let insert_results = self
            .ctx
            .mining_manager()
            .clone()
            .validate_and_insert_transaction_batch(&consensus, transactions, Priority::Low, Orphan::Allowed, RbfPolicy::Allowed)
            .await;

        for res in insert_results.iter() {
            match res {
                Ok(_) => {}
                Err(MiningManagerError::MempoolError(RuleError::RejectInvalid(transaction_id))) => {
                    // TODO: discuss a banning process
                    return Err(ProtocolError::MisbehavingPeer(format!("rejected invalid transaction {}", transaction_id)));
                }
                Err(MiningManagerError::MempoolError(RuleError::RejectSpamTransaction(_)))
                | Err(MiningManagerError::MempoolError(RuleError::RejectNonStandard(..))) => {
                    self.spam_counter += 1;
                    if self.spam_counter % 100 == 0 {
                        kaspa_core::warn!("Peer {} has shared {} spam/non-standard txs ({:?})", self.router, self.spam_counter, res);
                    }
                }
                Err(_) => {}
            }
        }

        self.ctx
            .broadcast_transactions(
                insert_results.into_iter().filter_map(|res| match res {
                    Ok(x) => Some(x.id()),
                    Err(_) => None,
                }),
                should_throttle,
            )
            .await;

        Ok(())
    }
}

// Flow listening to RequestTransactions messages, responding with the requested
// transactions if those are in the mempool.
// Missing transactions would be ignored
pub struct RequestTransactionsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestTransactionsFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestTransactionsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestTransactions)?;
            let tx_ids: Vec<_> = msg.try_into()?;
            for transaction_id in tx_ids {
                if let Some(mutable_tx) =
                    self.ctx.mining_manager().clone().get_transaction(transaction_id, TransactionQuery::TransactionsOnly).await
                {
                    // trace!("Send transaction {} to {}", mutable_tx.id(), self.router.identity());
                    self.router.enqueue(make_message!(Payload::Transaction, (&*mutable_tx.tx).into())).await?;
                } else {
                    // trace!("Send transaction id {} not found to {}", transaction_id, self.router.identity());
                    self.router
                        .enqueue(make_message!(
                            Payload::TransactionNotFound,
                            TransactionNotFoundMessage { id: Some(transaction_id.into()) }
                        ))
                        .await?;
                }
            }
        }
    }
}

/// If in the last 10 seconds we exceeded the TPS threshold, we will throttle tx relay
fn check_tx_throttling(throttling_state: &mut ThrottlingState, next_snapshot: P2pTxCountSample) {
    let snapshot_delta = &next_snapshot - &throttling_state.curr_snapshot;

    throttling_state.curr_snapshot = next_snapshot;

    if snapshot_delta.low_priority_tx_counts > 0 {
        let tps = 1000 * snapshot_delta.low_priority_tx_counts / snapshot_delta.elapsed_time.as_millis().max(1) as u64;
        if !throttling_state.should_throttle && tps > MAX_TPS_THRESHOLD {
            warn!("P2P tx relay threshold exceeded. Throttling relay. Current: {}, Max: {}", tps, MAX_TPS_THRESHOLD);
            throttling_state.should_throttle = true;
        } else if throttling_state.should_throttle && tps < MAX_TPS_THRESHOLD / 2 {
            warn!("P2P tx relay threshold back to normal. Current: {}, Max: {}", tps, MAX_TPS_THRESHOLD);
            throttling_state.should_throttle = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn create_snapshot(low_priority_tx_counts: u64, elapsed_time: u64) -> P2pTxCountSample {
        P2pTxCountSample { low_priority_tx_counts, elapsed_time: Duration::from_millis(elapsed_time) }
    }

    #[test]
    fn test_check_tx_throttling() {
        let mut elapsed_time = 0; // in milliseconds
        let mut p2p_tx_counts = 0;
        let mut throttling_state = ThrottlingState {
            should_throttle: false,
            last_checked_time: 0,
            curr_snapshot: create_snapshot(p2p_tx_counts, elapsed_time),
        };

        // Below threshold
        p2p_tx_counts += MAX_TPS_THRESHOLD / 3;
        elapsed_time += 1000;
        check_tx_throttling(&mut throttling_state, create_snapshot(p2p_tx_counts, elapsed_time));
        assert!(!throttling_state.should_throttle);

        // Still below threshold
        p2p_tx_counts += MAX_TPS_THRESHOLD;
        elapsed_time += 1000;
        check_tx_throttling(&mut throttling_state, create_snapshot(p2p_tx_counts, elapsed_time));
        assert!(!throttling_state.should_throttle);

        // Go above threshold. Note, this is not sensitive to tight bounds and can allow for up to (bps - 1) in excess of threshold
        // before triggering throttling.
        p2p_tx_counts += MAX_TPS_THRESHOLD + 1;
        elapsed_time += 1000;
        check_tx_throttling(&mut throttling_state, create_snapshot(p2p_tx_counts, elapsed_time));
        assert!(throttling_state.should_throttle);

        // Go below threshold but not enough to stop throttling
        // (need to be below threshold / 2 to stop throttling)
        p2p_tx_counts += MAX_TPS_THRESHOLD / 2;
        elapsed_time += 1000;
        check_tx_throttling(&mut throttling_state, create_snapshot(p2p_tx_counts, elapsed_time));
        assert!(throttling_state.should_throttle);

        // Go below threshold to stop throttling
        p2p_tx_counts += (MAX_TPS_THRESHOLD - 1) / 2;
        elapsed_time += 1000;
        check_tx_throttling(&mut throttling_state, create_snapshot(p2p_tx_counts, elapsed_time));
        assert!(!throttling_state.should_throttle);
    }
}
