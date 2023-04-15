//! Core server implementation for ClientAPI

use super::collector::{CollectorFromConsensus, CollectorFromIndex};
use crate::converter::{consensus::ConsensusConverter, index::IndexConverter};
use async_trait::async_trait;
use kaspa_consensus_core::{
    block::Block,
    coinbase::MinerData,
    config::Config,
    constants::MAX_SOMPI,
    tx::{Transaction, COINBASE_TRANSACTION_INDEX},
};
use kaspa_consensus_notify::{
    notifier::ConsensusNotifier,
    {connection::ConsensusChannelConnection, notification::Notification as ConsensusNotification},
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::{debug, info, trace, version::version, warn};
use kaspa_index_core::{
    connection::IndexChannelConnection, indexed_utxos::UtxoSetByScriptPublicKey, notification::Notification as IndexNotification,
    notifier::IndexNotifier,
};
use kaspa_mining::{manager::MiningManager, mempool::tx::Orphan};
use kaspa_notify::{
    collector::DynCollector,
    events::{EventSwitches, EventType, EVENT_TYPE_ARRAY},
    listener::ListenerId,
    notifier::Notifier,
    scope::Scope,
    subscriber::{Subscriber, SubscriptionManager},
};
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_rpc_core::{api::rpc::RpcApi, model::*, notify::connection::ChannelConnection, Notification, RpcError, RpcResult};
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script};
use kaspa_utils::channel::Channel;
use kaspa_utxoindex::api::DynUtxoIndexApi;
use std::{iter::once, ops::Deref, sync::Arc, vec};

/// A service implementing the Rpc API at kaspa_rpc_core level.
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
    consensus_manager: Arc<ConsensusManager>,
    notifier: Arc<Notifier<Notification, ChannelConnection>>,
    mining_manager: Arc<MiningManager>,
    flow_context: Arc<FlowContext>,
    utxoindex: DynUtxoIndexApi,
    config: Arc<Config>,
    consensus_converter: Arc<ConsensusConverter>,
    index_converter: Arc<IndexConverter>,
}

const RPC_CORE: &str = "rpc-core";

impl RpcCoreService {
    pub fn new(
        consensus_manager: Arc<ConsensusManager>,
        consensus_notifier: Arc<ConsensusNotifier>,
        index_notifier: Option<Arc<IndexNotifier>>,
        mining_manager: Arc<MiningManager>,
        flow_context: Arc<FlowContext>,
        utxoindex: DynUtxoIndexApi,
        config: Arc<Config>,
    ) -> Self {
        // Prepare consensus-notify objects
        let consensus_notify_channel = Channel::<ConsensusNotification>::default();
        let consensus_notify_listener_id =
            consensus_notifier.register_new_listener(ConsensusChannelConnection::new(consensus_notify_channel.sender()));

        // Prepare the rpc-core notifier objects
        let mut consensus_events: EventSwitches = EVENT_TYPE_ARRAY[..].into();
        consensus_events[EventType::UtxosChanged] = false;
        consensus_events[EventType::PruningPointUtxoSetOverride] = index_notifier.is_none();
        let consensus_converter = Arc::new(ConsensusConverter::new(consensus_manager.clone(), config.clone()));
        let consensus_collector =
            Arc::new(CollectorFromConsensus::new(consensus_notify_channel.receiver(), consensus_converter.clone()));
        let consensus_subscriber = Arc::new(Subscriber::new(consensus_events, consensus_notifier, consensus_notify_listener_id));

        let mut collectors: Vec<DynCollector<Notification>> = vec![consensus_collector];
        let mut subscribers = vec![consensus_subscriber];

        // Prepare index-processor objects if an IndexService is provided
        let index_converter = Arc::new(IndexConverter::new(config.clone()));
        if let Some(ref index_notifier) = index_notifier {
            let index_notify_channel = Channel::<IndexNotification>::default();
            let index_notify_listener_id =
                index_notifier.clone().register_new_listener(IndexChannelConnection::new(index_notify_channel.sender()));

            let index_events: EventSwitches = [EventType::UtxosChanged, EventType::PruningPointUtxoSetOverride].as_ref().into();
            let index_collector = Arc::new(CollectorFromIndex::new(index_notify_channel.receiver(), index_converter.clone()));
            let index_subscriber = Arc::new(Subscriber::new(index_events, index_notifier.clone(), index_notify_listener_id));

            collectors.push(index_collector);
            subscribers.push(index_subscriber);
        }

        // Create the rcp-core notifier
        let notifier = Arc::new(Notifier::new(EVENT_TYPE_ARRAY[..].into(), collectors, subscribers, 1, RPC_CORE));

        Self { consensus_manager, notifier, mining_manager, flow_context, utxoindex, config, consensus_converter, index_converter }
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

    fn get_utxo_set_by_script_public_key<'a>(&self, addresses: impl Iterator<Item = &'a RpcAddress>) -> UtxoSetByScriptPublicKey {
        self.utxoindex
            .as_ref()
            .unwrap()
            .read()
            .get_utxos_by_script_public_keys(addresses.map(pay_to_address_script).collect())
            .unwrap_or_default()
    }
}

#[async_trait]
impl RpcApi<ChannelConnection> for RpcCoreService {
    async fn submit_block_call(&self, request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse> {
        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;

        // TODO: consider adding an error field to SubmitBlockReport to document both the report and error fields
        let is_synced: bool = self.flow_context.hub().has_peers() && session.is_nearly_synced();

        if !self.config.allow_submit_block_when_not_synced && !is_synced {
            // error = "Block not submitted - node is not synced"
            return Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::IsInIBD) });
        }

        let try_block: RpcResult<Block> = (&request.block).try_into();
        if let Err(err) = &try_block {
            trace!("incoming SubmitBlockRequest with block conversion error: {}", err);
            // error = format!("Could not parse block: {0}", err)
            return Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) });
        }
        let block = try_block?;
        let hash = block.hash();

        if !request.allow_non_daa_blocks {
            let virtual_daa_score = session.get_virtual_daa_score();

            // A simple heuristic check which signals that the mined block is out of date
            // and should not be accepted unless user explicitly requests
            let daa_window_size = self.config.difficulty_window_size as u64;
            if virtual_daa_score > daa_window_size && block.header.daa_score < virtual_daa_score - daa_window_size {
                // error = format!("Block rejected. Reason: block DAA score {0} is too far behind virtual's DAA score {1}", block.header.daa_score, virtual_daa_score)
                return Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) });
            }
        }

        trace!("incoming SubmitBlockRequest for block {}", hash);
        match self.flow_context.add_block(session.deref(), block.clone()).await {
            Ok(_) => {
                info!("Accepted block {} via submit block", hash);
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Success })
            }
            Err(err) => {
                warn!("The RPC submitted block triggered an error: {}\nPrinting the full header for debug purposes:\n{:?}", err, err);
                // error = format!("Block rejected. Reason: {}", err))
                Ok(SubmitBlockResponse { report: SubmitBlockReport::Reject(SubmitBlockRejectReason::BlockInvalid) })
            }
        }
    }

    async fn get_block_template_call(&self, request: GetBlockTemplateRequest) -> RpcResult<GetBlockTemplateResponse> {
        trace!("incoming GetBlockTemplate request");

        // Make sure the pay address prefix matches the config network type
        if request.pay_address.prefix != self.config.prefix() {
            return Err(kaspa_addresses::AddressError::InvalidPrefix(request.pay_address.prefix.to_string()))?;
        }

        // Build block template
        let script_public_key = kaspa_txscript::pay_to_address_script(&request.pay_address);
        let extra_data = version().as_bytes().iter().chain(once(&(b'/'))).chain(&request.extra_data).cloned().collect::<Vec<_>>();
        let miner_data: MinerData = MinerData::new(script_public_key, extra_data);
        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;
        let block_template = self.mining_manager.get_block_template(session.deref(), &miner_data)?;

        // Check coinbase tx payload length
        if block_template.block.transactions[COINBASE_TRANSACTION_INDEX].payload.len() > self.config.max_coinbase_payload_len {
            return Err(RpcError::CoinbasePayloadLengthAboveMax(self.config.max_coinbase_payload_len));
        }

        let is_nearly_synced = self.config.is_nearly_synced(block_template.selected_parent_timestamp);
        Ok(GetBlockTemplateResponse {
            block: (&block_template.block).into(),
            is_synced: self.flow_context.hub().has_peers() && is_nearly_synced,
        })
    }

    async fn get_block_call(&self, request: GetBlockRequest) -> RpcResult<GetBlockResponse> {
        // TODO: test
        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;
        let mut block = session.get_block_even_if_header_only(request.hash)?;
        if !request.include_transactions {
            block.transactions = Arc::new(vec![]);
        }
        Ok(GetBlockResponse { block: self.consensus_converter.get_block(session.deref(), &block, request.include_transactions)? })
    }

    async fn get_blocks_call(&self, request: GetBlocksRequest) -> RpcResult<GetBlocksResponse> {
        // Validate that user didn't set include_transactions without setting include_blocks
        if !request.include_blocks && request.include_transactions {
            return Err(RpcError::InvalidGetBlocksRequest);
        }

        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;

        // If low_hash is empty - use genesis instead.
        let low_hash = match request.low_hash {
            Some(low_hash) => {
                // Make sure low_hash points to an existing and valid block
                session.deref().get_ghostdag_data(low_hash)?;
                low_hash
            }
            None => self.config.genesis.hash,
        };

        // Get hashes between low_hash and sink
        let sink_hash = session.get_sink();

        // We use +1 because low_hash is also returned
        // max_blocks MUST be >= mergeset_size_limit + 1
        let max_blocks = self.config.mergeset_size_limit as usize + 1;
        let (block_hashes, high_hash) = session.get_hashes_between(low_hash, sink_hash, max_blocks)?;

        // If the high hash is equal to sink it means get_hashes_between didn't skip any hashes, and
        // there's space to add the sink anticone, otherwise we cannot add the anticone because
        // there's no guarantee that all of the anticone root ancestors will be present.
        let sink_anticone = if high_hash == sink_hash { session.get_anticone(sink_hash)? } else { vec![] };
        // Prepend low hash to make it inclusive and append the sink anticone
        let block_hashes = once(low_hash).chain(block_hashes).chain(sink_anticone).collect::<Vec<_>>();
        let blocks = if request.include_blocks {
            block_hashes
                .iter()
                .cloned()
                .map(|hash| {
                    let mut block = session.get_block_even_if_header_only(hash)?;
                    if !request.include_transactions {
                        block.transactions = Arc::new(vec![]);
                    }
                    self.consensus_converter.get_block(session.deref(), &block, request.include_transactions)
                })
                .collect::<RpcResult<Vec<_>>>()
        } else {
            Ok(vec![])
        }?;
        Ok(GetBlocksResponse { block_hashes, blocks })
    }

    async fn get_info_call(&self, _request: GetInfoRequest) -> RpcResult<GetInfoResponse> {
        let is_nearly_synced = self.consensus_manager.consensus().session().await.is_nearly_synced();
        Ok(GetInfoResponse {
            p2p_id: self.flow_context.node_id.to_string(),
            mempool_size: self.mining_manager.transaction_count(true, false) as u64,
            server_version: version().to_string(),
            is_utxo_indexed: self.config.utxoindex,
            is_synced: self.flow_context.hub().has_peers() && is_nearly_synced,
            has_notify_command: true,
            has_message_id: true,
        })
    }

    async fn get_mempool_entry_call(&self, request: GetMempoolEntryRequest) -> RpcResult<GetMempoolEntryResponse> {
        let Some(transaction) = self.mining_manager.get_transaction(&request.transaction_id, !request.filter_transaction_pool, request.include_orphan_pool) else {
            return Err(RpcError::TransactionNotFound(request.transaction_id));
        };
        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;
        Ok(GetMempoolEntryResponse::new(self.consensus_converter.get_mempool_entry(session.deref(), &transaction)))
    }

    async fn get_mempool_entries_call(&self, request: GetMempoolEntriesRequest) -> RpcResult<GetMempoolEntriesResponse> {
        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;
        let (transactions, orphans) =
            self.mining_manager.get_all_transactions(!request.filter_transaction_pool, request.include_orphan_pool);
        let mempool_entries = transactions
            .iter()
            .chain(orphans.iter())
            .map(|transaction| self.consensus_converter.get_mempool_entry(session.deref(), transaction))
            .collect();
        Ok(GetMempoolEntriesResponse::new(mempool_entries))
    }

    async fn get_mempool_entries_by_addresses_call(
        &self,
        request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse> {
        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;
        let script_public_keys = request.addresses.iter().map(pay_to_address_script).collect();
        let grouped_txs = self.mining_manager.get_transactions_by_addresses(
            &script_public_keys,
            !request.filter_transaction_pool,
            request.include_orphan_pool,
        );
        let mempool_entries = grouped_txs
            .owners
            .iter()
            .map(|(script_public_key, owner_transactions)| {
                let address = extract_script_pub_key_address(script_public_key, self.config.prefix())
                    .expect("script public key is convertible into an address");
                self.consensus_converter.get_mempool_entries_by_address(
                    session.deref(),
                    address,
                    owner_transactions,
                    &grouped_txs.transactions,
                )
            })
            .collect();
        Ok(GetMempoolEntriesByAddressesResponse::new(mempool_entries))
    }

    async fn submit_transaction_call(&self, request: SubmitTransactionRequest) -> RpcResult<SubmitTransactionResponse> {
        let transaction: Transaction = (&request.transaction).try_into()?;
        let transaction_id = transaction.id();
        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;
        self.flow_context.add_transaction(session.deref(), transaction, Orphan::Allowed).await.map_err(|err| {
            let err = RpcError::RejectedTransaction(transaction_id, err.to_string());
            debug!("{err}");
            err
        })?;
        Ok(SubmitTransactionResponse::new(transaction_id))
    }

    async fn get_current_network_call(&self, _: GetCurrentNetworkRequest) -> RpcResult<GetCurrentNetworkResponse> {
        Ok(GetCurrentNetworkResponse::new(self.config.net))
    }

    async fn get_subnetwork_call(&self, _: GetSubnetworkRequest) -> RpcResult<GetSubnetworkResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_selected_tip_hash_call(&self, _: GetSelectedTipHashRequest) -> RpcResult<GetSelectedTipHashResponse> {
        Ok(GetSelectedTipHashResponse::new(self.consensus_manager.consensus().session().await.get_sink()))
    }

    async fn get_sink_blue_score_call(&self, _: GetSinkBlueScoreRequest) -> RpcResult<GetSinkBlueScoreResponse> {
        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;
        Ok(GetSinkBlueScoreResponse::new(session.get_ghostdag_data(session.get_sink())?.blue_score))
    }

    async fn get_virtual_chain_from_block_call(
        &self,
        request: GetVirtualChainFromBlockRequest,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        let consensus = self.consensus_manager.consensus();
        let session = consensus.session().await;
        let virtual_chain = session.get_virtual_chain_from_block(request.start_hash)?;
        let accepted_transaction_ids = if request.include_accepted_transaction_ids {
            self.consensus_converter.get_virtual_chain_accepted_transaction_ids(session.deref(), &virtual_chain)?
        } else {
            vec![]
        };
        Ok(GetVirtualChainFromBlockResponse::new(virtual_chain.removed, virtual_chain.added, accepted_transaction_ids))
    }

    async fn get_block_count_call(&self, _: GetBlockCountRequest) -> RpcResult<GetBlockCountResponse> {
        Ok(self.consensus_manager.consensus().session().await.get_sync_info())
    }

    async fn get_utxos_by_addresses_call(&self, request: GetUtxosByAddressesRequest) -> RpcResult<GetUtxosByAddressesResponse> {
        if !self.config.utxoindex {
            return Err(RpcError::NoUtxoIndex);
        }
        // TODO: discuss if the entry order is part of the method requirements
        //       (the current impl does not retain an entry order matching the request addresses order)
        let entry_map = self.get_utxo_set_by_script_public_key(request.addresses.iter());
        Ok(GetUtxosByAddressesResponse::new(self.index_converter.get_utxos_by_addresses_entries(&entry_map)))
    }

    async fn get_balance_by_address_call(&self, request: GetBalanceByAddressRequest) -> RpcResult<GetBalanceByAddressResponse> {
        if !self.config.utxoindex {
            return Err(RpcError::NoUtxoIndex);
        }
        let entry_map = self.get_utxo_set_by_script_public_key(once(&request.address));
        let balance = entry_map.values().flat_map(|x| x.values().map(|entry| entry.amount)).sum();
        Ok(GetBalanceByAddressResponse::new(balance))
    }

    async fn get_balances_by_addresses_call(
        &self,
        request: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse> {
        if !self.config.utxoindex {
            return Err(RpcError::NoUtxoIndex);
        }
        let entry_map = self.get_utxo_set_by_script_public_key(request.addresses.iter());
        let entries = request
            .addresses
            .iter()
            .map(|address| {
                let script_public_key = pay_to_address_script(address);
                let balance = entry_map.get(&script_public_key).map(|x| x.values().map(|entry| entry.amount).sum());
                RpcBalancesByAddressesEntry { address: address.to_owned(), balance }
            })
            .collect();
        Ok(GetBalancesByAddressesResponse::new(entries))
    }

    async fn get_coin_supply_call(&self, _: GetCoinSupplyRequest) -> RpcResult<GetCoinSupplyResponse> {
        if !self.config.utxoindex {
            return Err(RpcError::NoUtxoIndex);
        }
        let circulating_sompi =
            self.utxoindex.as_ref().unwrap().read().get_circulating_supply().map_err(|e| RpcError::General(e.to_string()))?;
        Ok(GetCoinSupplyResponse::new(MAX_SOMPI, circulating_sompi))
    }

    async fn ping_call(&self, _: PingRequest) -> RpcResult<PingResponse> {
        Ok(PingResponse {})
    }

    async fn get_headers_call(&self, _request: GetHeadersRequest) -> RpcResult<GetHeadersResponse> {
        Err(RpcError::NotImplemented)
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // UNIMPLEMENTED METHODS

    async fn get_peer_addresses_call(&self, _request: GetPeerAddressesRequest) -> RpcResult<GetPeerAddressesResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_connected_peer_info_call(&self, _request: GetConnectedPeerInfoRequest) -> RpcResult<GetConnectedPeerInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn add_peer_call(&self, _request: AddPeerRequest) -> RpcResult<AddPeerResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_dag_info_call(&self, _request: GetBlockDagInfoRequest) -> RpcResult<GetBlockDagInfoResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn resolve_finality_conflict_call(
        &self,
        _request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn shutdown_call(&self, _request: ShutdownRequest) -> RpcResult<ShutdownResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn ban_call(&self, _request: BanRequest) -> RpcResult<BanResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn unban_call(&self, _request: UnbanRequest) -> RpcResult<UnbanResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn estimate_network_hashes_per_second_call(
        &self,
        _request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_process_metrics_call(&self, _request: GetProcessMetricsRequest) -> RpcResult<GetProcessMetricsResponse> {
        Err(RpcError::NotImplemented)
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
