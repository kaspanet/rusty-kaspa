use async_trait::async_trait;
use consensus_core::api::DynConsensus;
use kaspa_core::{info, trace};
use p2p_lib::infra::{KaspadMessagePayloadEnumU8, Router, RouterApi};
use p2p_lib::pb::VersionMessage;
use p2p_lib::pb::{self, kaspad_message::Payload, KaspadMessage};
use p2p_lib::registry::{Flow, FlowRegistryApi, FlowTxTerminateChannelType, P2pConnection};
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use uuid::Uuid;

use crate::common::FlowError;
use crate::recv_payload;
use crate::v5::echo::EchoFlow;

pub struct FlowContext {
    /// For now, directly hold consensus
    pub consensus: DynConsensus,
}

impl FlowContext {
    pub fn new(consensus: DynConsensus) -> Self {
        Self { consensus }
    }

    pub fn get_consensus(&self) -> DynConsensus {
        self.consensus.clone()
    }
}

#[inline]
fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

impl FlowContext {
    async fn receive_version_flow(
        &self,
        router: &Arc<Router>,
        mut receiver: Receiver<KaspadMessage>,
    ) -> Result<VersionMessage, FlowError> {
        info!("starting receive version flow");
        let version_message = recv_payload!(receiver, Payload::Version)?;
        info!("accepted version massage: {version_message:?}");
        let verack_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Verack(pb::VerackMessage {})) };
        router.route_to_network(verack_message).await;
        Ok(version_message)
    }

    async fn send_version_flow(&self, router: &Arc<Router>, mut receiver: Receiver<KaspadMessage>) -> Result<(), FlowError> {
        info!("starting send version flow");
        // TODO: full and accurate version info
        let version_message = pb::VersionMessage {
            protocol_version: 5, // TODO: make a const
            services: 0,         // TODO: get number of live services
            timestamp: unix_now(),
            address: None,                          // TODO
            id: Vec::from(Uuid::new_v4().as_ref()), // TODO
            user_agent: String::new(),              // TODO
            disable_relay_tx: false,                // TODO: config/cmd?
            subnetwork_id: None,                    // Subnets are not currently supported
            network: "kaspa-mainnet".to_string(),   // TODO: get network from config
        };
        let version_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Version(version_message)) };
        router.route_to_network(version_message).await;
        let verack_message = recv_payload!(receiver, Payload::Verack)?;
        info!("accepted verack_message: {verack_message:?}");
        Ok(())
    }

    async fn ready_flow(&self, router: &Arc<Router>, mut receiver: Receiver<KaspadMessage>) -> Result<(), FlowError> {
        info!("starting ready flow");
        let sent_ready_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Ready(pb::ReadyMessage {})) };
        router.route_to_network(sent_ready_message).await;
        let recv_ready_message = recv_payload!(receiver, Payload::Ready)?;
        info!("accepted ready message: {recv_ready_message:?}");
        Ok(())
    }

    async fn handshake(
        &self,
        router: &Arc<Router>,
        version_receiver: Receiver<KaspadMessage>,
        verack_receiver: Receiver<KaspadMessage>,
    ) -> Result<VersionMessage, FlowError> {
        // Run both send and receive flows concurrently -- this is critical in order to avoid a handshake deadlock
        let (send_res, recv_res) =
            tokio::join!(self.send_version_flow(router, verack_receiver), self.receive_version_flow(router, version_receiver));
        send_res?;
        recv_res
    }
}

#[async_trait]
impl FlowRegistryApi for FlowContext {
    async fn initialize_flows(&self, connection: P2pConnection) -> Result<Vec<(Uuid, FlowTxTerminateChannelType)>, ()> {
        let router = connection.router.clone();
        // Subscribe to handshake messages
        let version_receiver = router.subscribe_to(vec![KaspadMessagePayloadEnumU8::Version]);
        let verack_receiver = router.subscribe_to(vec![KaspadMessagePayloadEnumU8::Verack]);
        let ready_receiver = router.subscribe_to(vec![KaspadMessagePayloadEnumU8::Ready]);

        // Make sure possibly pending handshake messages are rerouted to the newly registered flows
        router.reroute_to_flows().await;
        // Perform the initial handshake
        let version_message = match self.handshake(&router, version_receiver, verack_receiver).await {
            Ok(version_message) => version_message,
            Err(err) => {
                connection.handle_error(err.into()).await;
                return Err(());
            }
        };
        info!("peer protocol version: {}", version_message.protocol_version);

        // Subscribe to remaining messages and finalize (finalize will reroute all messages into flows)
        // TODO: register to all kaspa P2P flows here
        let echo_terminate = EchoFlow::new(router.clone()).await;
        trace!("finalizing flow subscriptions");
        router.finalize().await;

        if let Err(err) = self.ready_flow(&router, ready_receiver).await {
            connection.handle_error(err.into()).await;
            return Err(());
        }

        // Note: at this point receivers for handshake subscriptions
        // are dropped, thus effectively unsubscribing

        Ok(vec![echo_terminate])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use consensus_core::api::ConsensusApi;
    use p2p_lib::adaptor::{P2pAdaptor, P2pAdaptorApi};

    struct ConsensusMock;

    // TODO: use mockall?
    impl ConsensusApi for ConsensusMock {
        fn build_block_template(
            self: std::sync::Arc<Self>,
            _miner_data: consensus_core::coinbase::MinerData,
            _txs: Vec<consensus_core::tx::Transaction>,
        ) -> Result<consensus_core::block::BlockTemplate, consensus_core::errors::block::RuleError> {
            todo!()
        }

        fn validate_and_insert_block(
            self: std::sync::Arc<Self>,
            _block: consensus_core::block::Block,
            _update_virtual: bool,
        ) -> futures::future::BoxFuture<
            'static,
            consensus_core::errors::block::BlockProcessResult<consensus_core::blockstatus::BlockStatus>,
        > {
            todo!()
        }

        fn validate_mempool_transaction_and_populate(
            self: std::sync::Arc<Self>,
            _transaction: &mut consensus_core::tx::MutableTransaction,
        ) -> consensus_core::errors::tx::TxResult<()> {
            todo!()
        }

        fn calculate_transaction_mass(self: std::sync::Arc<Self>, _transaction: &consensus_core::tx::Transaction) -> u64 {
            todo!()
        }

        fn get_virtual_daa_score(self: std::sync::Arc<Self>) -> u64 {
            todo!()
        }

        fn modify_coinbase_payload(
            self: std::sync::Arc<Self>,
            _payload: Vec<u8>,
            _miner_data: &consensus_core::coinbase::MinerData,
        ) -> consensus_core::errors::coinbase::CoinbaseResult<Vec<u8>> {
            todo!()
        }
    }

    #[tokio::test]
    async fn test_p2p_handshake() {
        kaspa_core::log::try_init_logger("info");

        let address1 = String::from("[::1]:50053");
        let adaptor1 = P2pAdaptor::listen(address1.clone(), Arc::new(FlowContext::new(Arc::new(ConsensusMock {})))).await.unwrap();

        let address2 = String::from("[::1]:50054");
        let _adaptor2 = P2pAdaptor::listen(address2.clone(), Arc::new(FlowContext::new(Arc::new(ConsensusMock {})))).await.unwrap();

        // Initiate the connection from `adaptor1` (outbound) to `adaptor2` (inbound)
        // NOTE: a minimal scheme prefix `"://"` must be added for the client-side connect logic
        adaptor1.connect_peer(String::from("://[::1]:50054")).await;

        // TODO: find a better mechanism to sync on handshake completion
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;

        // For now assert the handshake by checking the peer exists (since peer is removed on handshake error)
        assert_eq!(adaptor1.get_all_peer_ids().len(), 1, "handshake failed -- outbound peer is missing");
        // TODO: uncomment when fixing the issue that inbound peers are not registered in the peers list
        // assert_eq!(adaptor2.get_all_peer_ids().len(), 1, "handshake failed -- inbound peer is missing");
    }
}
