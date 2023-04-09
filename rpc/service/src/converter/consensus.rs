use async_trait::async_trait;
use kaspa_consensus_core::{
    api::ConsensusApi,
    block::Block,
    config::Config,
    hashing::tx::hash,
    header::Header,
    tx::{Transaction, TransactionInput, TransactionOutput},
};
use kaspa_consensus_notify::notification::{self as consensus_notify, Notification as ConsensusNotification};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_math::Uint256;
use kaspa_notify::converter::Converter;
use kaspa_rpc_core::{
    BlockAddedNotification, Notification, RpcBlock, RpcBlockVerboseData, RpcError, RpcHash, RpcResult, RpcTransaction,
    RpcTransactionInput, RpcTransactionOutput, RpcTransactionOutputVerboseData, RpcTransactionVerboseData,
};
use kaspa_txscript::{extract_script_pub_key_address, script_class::ScriptClass};
use std::{fmt::Debug, ops::Deref, sync::Arc};

/// Conversion of consensus_core to rpc_core structures
pub struct ConsensusConverter {
    consensus_manager: Arc<ConsensusManager>,
    config: Arc<Config>,
}

impl ConsensusConverter {
    pub fn new(consensus_manager: Arc<ConsensusManager>, config: Arc<Config>) -> Self {
        Self { consensus_manager, config }
    }

    /// Returns the proof-of-work difficulty as a multiple of the minimum difficulty using
    /// the passed bits field from the header of a block.
    pub fn get_difficulty_ratio(&self, bits: u32) -> f64 {
        // The minimum difficulty is the max possible proof-of-work limit bits
        // converted back to a number. Note this is not the same as the proof of
        // work limit directly because the block difficulty is encoded in a block
        // with the compact form which loses precision.
        let _target = Uint256::from_compact_target_bits(bits);

        // TODO: use Uint265::to_f64() when available
        // self.config.params.pow_max.to_f64() / target.to_f64()
        1.0
    }

    /// Converts a consensus [`Block`] into an [`RpcBlock`], optionally including transaction verbose data.
    ///
    /// _GO-KASPAD: PopulateBlockWithVerboseData_
    pub fn get_block(
        &self,
        consensus: &dyn ConsensusApi,
        block: &Block,
        include_transaction_verbose_data: bool,
    ) -> RpcResult<RpcBlock> {
        let hash = block.hash();
        let block_info = consensus.get_block_info(hash)?;
        if !block_info.block_status.is_valid() {
            return Err(RpcError::InvalidBlock(hash));
        }
        let children = consensus.get_block_children(hash).unwrap_or_default();
        let is_chain_block = consensus.is_chain_block(hash)?;
        let verbose_data = Some(RpcBlockVerboseData {
            hash,
            difficulty: self.get_difficulty_ratio(block.header.bits),
            selected_parent_hash: block_info.selected_parent,
            transaction_ids: block.transactions.iter().map(|x| x.id()).collect(),
            is_header_only: block_info.block_status.is_header_only(),
            blue_score: block_info.blue_score,
            children_hashes: (*children).clone(),
            merge_set_blues_hashes: block_info.mergeset_blues,
            merge_set_reds_hashes: block_info.mergeset_reds,
            is_chain_block,
        });

        let transactions = block
            .transactions
            .iter()
            .map(|x| self.get_transaction(consensus, x, Some(&block.header), include_transaction_verbose_data))
            .collect::<RpcResult<Vec<RpcTransaction>>>()?;

        Ok(RpcBlock { header: (*block.header).clone(), transactions, verbose_data })
    }

    /// Converts a consensus [`Transaction`] into an [`RpcTransaction`], optionally including verbose data.
    ///
    /// _GO-KASPAD: PopulateTransactionWithVerboseData
    pub fn get_transaction(
        &self,
        consensus: &dyn ConsensusApi,
        transaction: &Transaction,
        header: Option<&Header>,
        include_verbose_data: bool,
    ) -> RpcResult<RpcTransaction> {
        if include_verbose_data {
            let verbose_data = Some(RpcTransactionVerboseData {
                transaction_id: transaction.id(),
                hash: hash(transaction),
                mass: consensus.calculate_transaction_mass(transaction),
                // TODO: make block_hash an option
                block_hash: header.map_or_else(RpcHash::default, |x| x.hash),
                block_time: header.map_or(0, |x| x.timestamp),
            });
            Ok(RpcTransaction {
                version: transaction.version,
                inputs: transaction.inputs.iter().map(|x| self.get_transaction_input(x)).collect(),
                outputs: transaction.outputs.iter().map(|x| self.get_transaction_output(x)).collect(),
                lock_time: transaction.lock_time,
                subnetwork_id: transaction.subnetwork_id.clone(),
                gas: transaction.gas,
                payload: transaction.payload.clone(),
                verbose_data,
            })
        } else {
            Ok(transaction.into())
        }
    }

    fn get_transaction_input(&self, input: &TransactionInput) -> RpcTransactionInput {
        input.into()
    }

    fn get_transaction_output(&self, output: &TransactionOutput) -> RpcTransactionOutput {
        let script_public_key_type = ScriptClass::from_script(&output.script_public_key);
        let address = extract_script_pub_key_address(&output.script_public_key, self.config.prefix()).ok();
        let verbose_data =
            address.map(|address| RpcTransactionOutputVerboseData { script_public_key_type, script_public_key_address: address });
        RpcTransactionOutput { value: output.value, script_public_key: output.script_public_key.clone(), verbose_data }
    }
}

#[async_trait]
impl Converter for ConsensusConverter {
    type Incoming = ConsensusNotification;
    type Outgoing = Notification;

    async fn convert(&self, incoming: ConsensusNotification) -> Notification {
        match incoming {
            consensus_notify::Notification::BlockAdded(msg) => {
                let consensus = self.consensus_manager.consensus();
                let session = consensus.session().await;

                // If get_block fails, rely on the infallible From implementation which will lack verbose data
                let block = Arc::new(self.get_block(session.deref(), &msg.block, true).unwrap_or_else(|_| (&msg.block).into()));

                Notification::BlockAdded(BlockAddedNotification { block })
            }
            _ => (&incoming).into(),
        }
    }
}

impl Debug for ConsensusConverter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsensusConverter").field("consensus_manager", &"").field("config", &self.config).finish()
    }
}
