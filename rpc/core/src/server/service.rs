//! Core server implementation for ClientAPI

use super::collector::{ConsensusCollector, ConsensusNotificationReceiver};
use crate::{
    api::rpc::RpcApi,
    model::*,
    notify::{
        channel::NotificationChannel,
        listener::{ListenerID, ListenerReceiverSide, ListenerUtxoNotificationFilterSetting},
        notifier::Notifier,
    },
    FromRpcHex, Notification, NotificationType, RpcError, RpcResult,
};
use async_trait::async_trait;
use consensus_core::{
    api::DynConsensus,
    block::Block,
    coinbase::MinerData,
    tx::{ScriptPublicKey, ScriptVec},
};
use hashes::Hash;
use kaspa_core::trace;
use std::{
    str::FromStr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
    vec,
};

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
    notifier: Arc<Notifier>,
}

impl RpcCoreService {
    pub fn new(consensus: DynConsensus, consensus_recv: ConsensusNotificationReceiver) -> Self {
        // TODO: instead of getting directly a DynConsensus, rely on some Context equivalent
        //       See app\rpc\rpccontext\context.go
        // TODO: the channel receiver should be obtained by registering to a consensus notification service

        let collector = Arc::new(ConsensusCollector::new(consensus_recv));

        // TODO: Some consensus-compatible subscriber could be provided here
        let notifier = Arc::new(Notifier::new(Some(collector), None, ListenerUtxoNotificationFilterSetting::All));

        Self { consensus, notifier }
    }

    pub fn start(&self) {
        self.notifier.clone().start();
    }

    pub async fn stop(&self) -> RpcResult<()> {
        self.notifier.clone().stop().await?;
        Ok(())
    }

    pub fn notifier(&self) -> Arc<Notifier> {
        self.notifier.clone()
    }
}

#[async_trait]
impl RpcApi for RpcCoreService {
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
        self.notifier.clone().notify(Arc::new(Notification::BlockAdded(BlockAddedNotification { block: rpc_block }))).unwrap();

        // Emit a NewBlockTemplate notification
        self.notifier.clone().notify(Arc::new(Notification::NewBlockTemplate(NewBlockTemplateNotification {}))).unwrap();

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
        let block_template = self.consensus.clone().build_block_template(miner_data, vec![]);

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
        })
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id and a channel receiver.
    fn register_new_listener(&self, channel: Option<NotificationChannel>) -> ListenerReceiverSide {
        self.notifier.register_new_listener(channel)
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener and drop its channel.
    async fn unregister_listener(&self, id: ListenerID) -> RpcResult<()> {
        self.notifier.unregister_listener(id)?;
        Ok(())
    }

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        self.notifier.start_notify(id, notification_type)?;
        Ok(())
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        self.notifier.stop_notify(id, notification_type)?;
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
