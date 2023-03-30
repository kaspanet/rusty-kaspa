use crate::{
    flow_context::{FlowContext, RequestScope},
    flow_trait::Flow,
};
use itertools::Itertools;
use kaspa_consensus_core::tx::{Transaction, TransactionId};
use kaspa_mining::{
    errors::MiningManagerError,
    mempool::{
        errors::RuleError,
        tx::{Orphan, Priority},
    },
};
use kaspa_p2p_lib::{
    common::{ProtocolError, DEFAULT_TIMEOUT},
    dequeue, make_message,
    pb::{kaspad_message::Payload, RequestTransactionsMessage, TransactionNotFoundMessage},
    IncomingRoute, Router,
};
use std::{collections::VecDeque, sync::Arc};
use tokio::time::timeout;

pub type RelayInvMessage = Vec<TransactionId>;

/// Encapsulates an incoming invs route which also receives data locally
pub struct TwoWayIncomingRoute {
    incoming_route: IncomingRoute,
    indirect_invs: VecDeque<Vec<TransactionId>>,
}

impl TwoWayIncomingRoute {
    pub fn new(incoming_route: IncomingRoute) -> Self {
        Self { incoming_route, indirect_invs: VecDeque::new() }
    }

    pub fn enqueue_indirect_inv(&mut self, ids: Vec<TransactionId>) {
        self.indirect_invs.push_back(ids)
    }

    pub async fn dequeue(&mut self) -> Result<RelayInvMessage, ProtocolError> {
        if let Some(inv) = self.indirect_invs.pop_front() {
            Ok(inv)
        } else {
            let msg = dequeue!(self.incoming_route, Payload::InvTransactions)?;
            Ok(msg.try_into()?)
        }
    }
}

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
    invs_route: TwoWayIncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RelayTransactionsFlow {
    fn name(&self) -> &'static str {
        "RELAY_TXS"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RelayTransactionsFlow {
    #[allow(dead_code)]
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, invs_route: TwoWayIncomingRoute::new(incoming_route) }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            // Loop over incoming block inv messages
            let inv = self.invs_route.dequeue().await?;

            // Transaction relay is disabled if the node is out of sync and thus not mining
            if self.ctx.is_ibd_running() {
                continue;
            }

            let requests = self.request_transactions(inv).await?;
            self.receive_transactions(requests).await?;
        }
    }

    async fn request_transactions(
        &self,
        transaction_ids: Vec<TransactionId>,
    ) -> Result<Vec<RequestScope<TransactionId>>, ProtocolError> {
        // Build a vector with the transaction ids unknown in the mempool and not already requested
        // by another peer
        let requests: Vec<_> = transaction_ids
            .iter()
            .filter_map(|transaction_id| {
                if !self.is_known_transaction(transaction_id) {
                    self.ctx.try_adding_transaction_request(transaction_id.to_owned())
                } else {
                    None
                }
            })
            .collect();

        // Request the transactions
        if !requests.is_empty() {
            // TODO: determine if there should be a limit to the number of ids per message
            self.router
                .enqueue(make_message!(
                    Payload::RequestTransactions,
                    RequestTransactionsMessage { ids: requests.iter().map(|x| x.req.into()).collect() }
                ))
                .await?;
        }

        Ok(requests)
    }

    fn is_known_transaction(&self, transaction_id: &TransactionId) -> bool {
        // Ask the transaction memory pool if the transaction is known
        // to it in any form (main pool or orphan).
        self.ctx.mining_manager().get_transaction(transaction_id, true, true).is_some()
    }

    /// Returns the next Transaction or TransactionNotFound message in incoming_route,
    /// returning only one of the message types at a time.
    ///
    /// Populates the invs_route queue with any inv messages that meanwhile arrive.
    async fn read_response(&mut self) -> Result<Response, ProtocolError> {
        loop {
            return match timeout(DEFAULT_TIMEOUT, self.invs_route.incoming_route.recv()).await {
                Ok(op) => {
                    if let Some(msg) = op {
                        match msg.payload {
                            Some(Payload::InvTransactions(payload)) => {
                                self.invs_route.enqueue_indirect_inv(payload.try_into()?);
                                continue;
                            }
                            Some(Payload::Transaction(payload)) => Ok(Response::Transaction(payload.try_into()?)),
                            Some(Payload::TransactionNotFound(payload)) => Ok(Response::NotFound(payload.try_into()?)),
                            _ => Err(ProtocolError::UnexpectedMessage(
                                stringify!(Payload::InvTransactions | Payload::Transaction | Payload::TransactionNotFound),
                                msg.payload.as_ref().map(|v| v.into()),
                            )),
                        }
                    } else {
                        Err(ProtocolError::ConnectionClosed)
                    }
                }
                Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
            };
        }
    }

    async fn receive_transactions(&mut self, requests: Vec<RequestScope<TransactionId>>) -> Result<(), ProtocolError> {
        for requested_id in requests.iter().map(|x| x.req.to_owned()) {
            let response = self.read_response().await?;
            let transaction_id = response.transaction_id();
            if transaction_id != requested_id {
                return Err(ProtocolError::OtherOwned(format!(
                    "requested transaction id {} but got transaction {}",
                    requested_id, transaction_id
                )));
            }
            let Response::Transaction(transaction) = response else { continue; };
            let result = self.ctx.mining_manager().validate_and_insert_transaction(transaction, Priority::Low, Orphan::Allowed);
            match result {
                Ok(accepted_transactions) => {
                    let transaction_ids = accepted_transactions.iter().map(|x| x.id()).collect_vec();
                    self.ctx.broadcast_transactions(transaction_ids).await?;
                }
                Err(MiningManagerError::MempoolError(err)) => {
                    if let RuleError::RejectInvalid(_) = err {
                        // TODO: discuss a banning process
                        return Err(ProtocolError::MisbehavingPeer(format!("rejected invalid transaction {}", transaction_id)));
                    }
                    continue;
                }
                Err(_) => {}
            }
        }
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
    fn name(&self) -> &'static str {
        "REQUEST_TXS"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestTransactionsFlow {
    #[allow(dead_code)]
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestTransactions)?;
            let tx_ids: Vec<_> = msg.try_into()?;
            for transaction_id in tx_ids {
                if let Some(mutable_tx) = self.ctx.mining_manager().get_transaction(&transaction_id, true, false) {
                    self.router.enqueue(make_message!(Payload::Transaction, (&*mutable_tx.tx).into())).await?;
                } else {
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
