//! Core server implementation for ClientAPI

use super::collector::{CollectorFromConsensus, CollectorFromIndex};
use crate::{api::rpc::RpcApi, model::*, notify::connection::ChannelConnection, FromRpcHex, Notification, RpcError, RpcResult};
use async_trait::async_trait;
use consensus_core::{
    api::DynConsensus,
    block::Block,
    coinbase::MinerData,
    tx::{ScriptPublicKey, ScriptVec},
};
use consensus_notify::{
    service::NotifyService,
    {connection::ConsensusChannelConnection, notification::Notification as ConsensusNotification},
};
use hashes::Hash;
use kaspa_core::trace;
use kaspa_index_processor::{
    connection::IndexChannelConnection, notification::Notification as IndexNotification, service::IndexService,
};
use kaspa_notify::{
    collector::DynCollector,
    events::{EventSwitches, EventType, EVENT_TYPE_ARRAY},
    listener::ListenerId,
    notifier::{Notifier, Notify},
    scope::Scope,
    subscriber::{Subscriber, SubscriptionManager},
};
use kaspa_utils::channel::Channel;
use std::{
    str::FromStr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
    vec,
};
use utxoindex::api::DynUtxoIndexApi;

/// A service implementing the Rpc API at rpc_core level.
///
/// Collects notifications from the consensus and forwards them to
/// actual protocol-featured services. Thanks to the subscription pattern,
/// notifications are sent to the registered services only if the actually
/// need them.
///
/// ### Implementation notes
///
/// This was designed to have a unique instance in the whole application,
/// though multiple instances could coexist safely.
///
/// Any lower-level service providing an actual protocol, like gPRC should
/// register into this instance in order to get notifications. The data flow
/// from this instance to registered services and backwards should occur
/// by adding respectively to the registered service a Collector and a
/// Subscriber.
pub struct RpcCoreService {
    consensus: DynConsensus,
    notifier: Arc<Notifier<Notification, ChannelConnection>>,
    index_service: Option<Arc<IndexService>>,
}

const RPC_CORE: &str = "rpc-core";

impl RpcCoreService {
    pub fn new(consensus: DynConsensus, notify_service: Arc<NotifyService>, index_service: Option<Arc<IndexService>>) -> Self {
        // TODO: instead of getting directly a DynConsensus, rely on some Context equivalent
        //       See app\rpc\rpccontext\context.go

        // Prepare consensus-notify objects
        let consensus_notify_channel = Channel::<ConsensusNotification>::default();
        let consensus_notify_listener_id =
            notify_service.notifier().register_new_listener(ConsensusChannelConnection::new(consensus_notify_channel.sender()));

        // Prepare the rpc-core notifier objects
        let mut consensus_events: EventSwitches = EVENT_TYPE_ARRAY[..].into();
        consensus_events[EventType::UtxosChanged] = false;
        consensus_events[EventType::PruningPointUtxoSetOverride] = index_service.is_none();
        let consensus_collector = Arc::new(CollectorFromConsensus::new(consensus_notify_channel.receiver()));
        let consensus_subscriber =
            Arc::new(Subscriber::new(consensus_events, notify_service.notifier(), consensus_notify_listener_id));

        let mut collectors: Vec<DynCollector<Notification>> = vec![consensus_collector];
        let mut subscribers = vec![consensus_subscriber];

        // Prepare index-processor objects if an IndexService is provided
        if let Some(ref index_service) = index_service {
            let index_notify_channel = Channel::<IndexNotification>::default();
            let index_notify_listener_id =
                index_service.notifier().register_new_listener(IndexChannelConnection::new(index_notify_channel.sender()));

            let index_events: EventSwitches = [EventType::UtxosChanged, EventType::PruningPointUtxoSetOverride].as_ref().into();
            let index_collector = Arc::new(CollectorFromIndex::new(index_notify_channel.receiver()));
            let index_subscriber = Arc::new(Subscriber::new(index_events, index_service.notifier(), index_notify_listener_id));

            collectors.push(index_collector);
            subscribers.push(index_subscriber);
        }

        // Create the rcp-core notifier
        let notifier = Arc::new(Notifier::new(EVENT_TYPE_ARRAY[..].into(), collectors, subscribers, 1, RPC_CORE));

        Self { consensus, notifier, index_service }
    }

    pub fn start(&self) {
        self.notifier().start();
    }

    pub async fn stop(&self) -> RpcResult<()> {
        self.notifier().stop().await?;
        Ok(())
    }

    #[inline(always)]
    pub fn notifier(&self) -> Arc<Notifier<Notification, ChannelConnection>> {
        self.notifier.clone()
    }

    #[inline(always)]
    pub fn utxoindex(&self) -> DynUtxoIndexApi {
        self.index_service.as_ref().and_then(|x| x.utxoindex())
    }
}

#[async_trait]
impl RpcApi<ChannelConnection> for RpcCoreService {
    async fn submit_block_call(&self, request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse> {
        let try_block: RpcResult<Block> = (&request.block).try_into();
        if let Err(ref err) = try_block {
            trace!("incoming SubmitBlockRequest with block conversion error: {}", err);
        }
        let block = try_block?;

        // We recreate a RpcBlock for the BlockAdded notification.
        // This guaranties that we have the right hash.
        // TODO: remove it when consensus emit a BlockAdded notification.
        let rpc_block: RpcBlock = (&block).into();

        trace!("incoming SubmitBlockRequest for block {}", block.header.hash);

        let result = match self.consensus.clone().validate_and_insert_block(block, true).await {
            Ok(_) => Ok(SubmitBlockResponse { report: SubmitBlockReport::Success }),
            Err(err) => {
                trace!("submit block error: {}", err);
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) })
            } // TODO: handle also the IsInIBD reject reason
        };

        // Notify about new added block
        // TODO: let consensus emit this notification through an event channel
        self.notifier.notify(Notification::BlockAdded(BlockAddedNotification { block: Arc::new(rpc_block) })).unwrap();

        // Emit a NewBlockTemplate notification
        self.notifier.notify(Notification::NewBlockTemplate(NewBlockTemplateNotification {})).unwrap();

        result
    }

    async fn get_block_template_call(&self, request: GetBlockTemplateRequest) -> RpcResult<GetBlockTemplateResponse> {
        trace!("incoming GetBlockTemplate request");

        // TODO: Replace this hack by a call to build the script (some txscript.PayToAddrScript(payAddress) equivalent).
        //       See app\rpc\rpchandlers\get_block_template.go HandleGetBlockTemplate
        const ADDRESS_PUBLIC_KEY_SCRIPT_PUBLIC_KEY_VERSION: u16 = 0;
        const OP_CHECK_SIG: u8 = 172;
        let mut script_addr = request.pay_address.payload.clone();
        let mut pay_to_pub_key_script = Vec::with_capacity(34);
        pay_to_pub_key_script.push(u8::try_from(script_addr.len()).unwrap());
        pay_to_pub_key_script.append(&mut script_addr);
        pay_to_pub_key_script.push(OP_CHECK_SIG);

        let script = ScriptVec::from_vec(pay_to_pub_key_script);

        let script_public_key = ScriptPublicKey::new(ADDRESS_PUBLIC_KEY_SCRIPT_PUBLIC_KEY_VERSION, script);
        let miner_data: MinerData = MinerData::new(script_public_key, request.extra_data);
        // TODO: handle error properly when managed through mining manager
        let block_template = self.consensus.clone().build_block_template(miner_data, vec![]).unwrap();

        Ok((&block_template).into())
    }

    async fn get_block_call(&self, req: GetBlockRequest) -> RpcResult<GetBlockResponse> {
        // TODO: Remove the following test when consensus is used to fetch data

        // This is a test to simulate a consensus error
        if req.hash.as_bytes()[0] == 0 {
            return Err(RpcError::General(format!("Block {0} not found", req.hash)));
        }

        // TODO: query info from consensus and use it to build the response
        Ok(GetBlockResponse { block: create_dummy_rpc_block() })
    }

    async fn get_info_call(&self, _req: GetInfoRequest) -> RpcResult<GetInfoResponse> {
        // TODO: query info from consensus and use it to build the response
        Ok(GetInfoResponse {
            p2p_id: "test".to_string(),
            mempool_size: 1,
            server_version: "0.12.8".to_string(),
            is_utxo_indexed: false,
            is_synced: false,
            has_notify_command: true,
            has_message_id: true,
        })
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // UNIMPLEMENTED METHODS

    async fn get_current_network_call(&self, _request: GetCurrentNetworkRequest) -> RpcResult<GetCurrentNetworkResponse> {
        unimplemented!();
    }

    async fn get_peer_addresses_call(&self, _request: GetPeerAddressesRequest) -> RpcResult<GetPeerAddressesResponse> {
        unimplemented!();
    }

    async fn get_selected_tip_hash_call(&self, _request: GetSelectedTipHashRequest) -> RpcResult<GetSelectedTipHashResponse> {
        unimplemented!();
    }

    async fn get_mempool_entry_call(&self, _request: GetMempoolEntryRequest) -> RpcResult<GetMempoolEntryResponse> {
        unimplemented!();
    }

    async fn get_mempool_entries_call(&self, _request: GetMempoolEntriesRequest) -> RpcResult<GetMempoolEntriesResponse> {
        unimplemented!();
    }

    async fn get_connected_peer_info_call(&self, _request: GetConnectedPeerInfoRequest) -> RpcResult<GetConnectedPeerInfoResponse> {
        unimplemented!();
    }

    async fn add_peer_call(&self, _request: AddPeerRequest) -> RpcResult<AddPeerResponse> {
        unimplemented!();
    }

    async fn submit_transaction_call(&self, _request: SubmitTransactionRequest) -> RpcResult<SubmitTransactionResponse> {
        unimplemented!();
    }

    async fn get_subnetwork_call(&self, _request: GetSubnetworkRequest) -> RpcResult<GetSubnetworkResponse> {
        unimplemented!();
    }

    async fn get_virtual_chain_from_block_call(
        &self,
        _request: GetVirtualChainFromBlockRequest,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        unimplemented!();
    }

    async fn get_blocks_call(&self, _request: GetBlocksRequest) -> RpcResult<GetBlocksResponse> {
        unimplemented!();
    }

    async fn get_block_count_call(&self, _request: GetBlockCountRequest) -> RpcResult<GetBlockCountResponse> {
        unimplemented!();
    }

    async fn get_block_dag_info_call(&self, _request: GetBlockDagInfoRequest) -> RpcResult<GetBlockDagInfoResponse> {
        unimplemented!();
    }

    async fn resolve_finality_conflict_call(
        &self,
        _request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse> {
        unimplemented!();
    }

    async fn shutdown_call(&self, _request: ShutdownRequest) -> RpcResult<ShutdownResponse> {
        unimplemented!();
    }

    async fn get_headers_call(&self, _request: GetHeadersRequest) -> RpcResult<GetHeadersResponse> {
        unimplemented!();
    }

    async fn get_balance_by_address_call(&self, _request: GetBalanceByAddressRequest) -> RpcResult<GetBalanceByAddressResponse> {
        //TODO: use self.utxoindex for this
        unimplemented!();
    }

    async fn get_balances_by_addresses_call(
        &self,
        _addresses: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse> {
        unimplemented!();
    }

    async fn get_utxos_by_addresses_call(&self, _addresses: GetUtxosByAddressesRequest) -> RpcResult<GetUtxosByAddressesResponse> {
        //TODO: use self.utxoindex for this
        unimplemented!();
    }

    async fn get_sink_blue_score_call(&self, _request: GetSinkBlueScoreRequest) -> RpcResult<GetSinkBlueScoreResponse> {
        unimplemented!();
    }

    async fn ban_call(&self, _request: BanRequest) -> RpcResult<BanResponse> {
        unimplemented!();
    }

    async fn unban_call(&self, _request: UnbanRequest) -> RpcResult<UnbanResponse> {
        unimplemented!();
    }

    async fn estimate_network_hashes_per_second_call(
        &self,
        _request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse> {
        unimplemented!();
    }

    async fn get_mempool_entries_by_addresses_call(
        &self,
        _request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse> {
        unimplemented!();
    }

    async fn get_coin_supply_call(&self, _request: GetCoinSupplyRequest) -> RpcResult<GetCoinSupplyResponse> {
        unimplemented!();
    }

    async fn ping_call(&self, _request: PingRequest) -> RpcResult<PingResponse> {
        Ok(PingResponse {})
    }

    async fn get_process_metrics_call(&self, _request: GetProcessMetricsRequest) -> RpcResult<GetProcessMetricsResponse> {
        unimplemented!();
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id identifying it.
    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId {
        self.notifier.register_new_listener(connection)
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener, unregister the id and its associated connection.
    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        self.notifier.unregister_listener(id)?;
        Ok(())
    }

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.notifier.clone().start_notify(id, scope).await?;
        Ok(())
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.notifier.clone().stop_notify(id, scope).await?;
        Ok(())
    }
}

// TODO: Remove the following function when consensus is used to fetch data
fn create_dummy_rpc_block() -> RpcBlock {
    let sel_parent_hash = Hash::from_str("5963be67f12da63004ce1baceebd7733c4fb601b07e9b0cfb447a3c5f4f3c4f0").unwrap();
    RpcBlock {
        header: RpcHeader {
            hash: Hash::from_str("8270e63a0295d7257785b9c9b76c9a2efb7fb8d6ac0473a1bff1571c5030e995").unwrap(),
            version: 1,
            parents_by_level: vec![],
            hash_merkle_root: Hash::from_str("4b5a041951c4668ecc190c6961f66e54c1ce10866bef1cf1308e46d66adab270").unwrap(),
            accepted_id_merkle_root: Hash::from_str("1a1310d49d20eab15bf62c106714bdc81e946d761701e81fabf7f35e8c47b479").unwrap(),
            utxo_commitment: Hash::from_str("e7cdeaa3a8966f3fff04e967ed2481615c76b7240917c5d372ee4ed353a5cc15").unwrap(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64,
            bits: 1,
            nonce: 1234,
            daa_score: 123456,
            blue_work: RpcBlueWorkType::from_rpc_hex("1234567890abcdef").unwrap(),
            pruning_point: Hash::from_str("7190c08d42a0f7994b183b52e7ef2f99bac0b91ef9023511cadf4da3a2184b16").unwrap(),
            blue_score: 12345678901,
        },
        transactions: vec![],
        verbose_data: Some(RpcBlockVerboseData {
            hash: Hash::from_str("8270e63a0295d7257785b9c9b76c9a2efb7fb8d6ac0473a1bff1571c5030e995").unwrap(),
            difficulty: 5678.0,
            selected_parent_hash: sel_parent_hash,
            transaction_ids: vec![],
            is_header_only: true,
            blue_score: 98765,
            children_hashes: vec![],
            merge_set_blues_hashes: vec![],
            merge_set_reds_hashes: vec![],
            is_chain_block: true,
        }),
    }
}
