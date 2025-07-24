use async_trait::async_trait;
use kaspa_addresses::{Address, AddressError};
use kaspa_consensus_core::{
    acceptance_data::MergesetBlockAcceptanceData,
    block::Block,
    config::Config,
    hashing::tx::hash,
    header::Header,
    tx::{
        MutableTransaction, SignableTransaction, Transaction, TransactionId, TransactionInput, TransactionOutput,
        TransactionQueryResult, TransactionType, UtxoEntry,
    },
    ChainPath,
};
use kaspa_consensus_notify::notification::{self as consensus_notify, Notification as ConsensusNotification};
use kaspa_consensusmanager::{ConsensusManager, ConsensusProxy};
use kaspa_hashes::Hash;
use kaspa_math::Uint256;
use kaspa_mining::model::{owner_txs::OwnerTransactions, TransactionIdSet};
use kaspa_notify::converter::Converter;
use kaspa_rpc_core::{
    BlockAddedNotification, Notification, RpcAcceptanceData, RpcAcceptanceDataVerbosity, RpcAcceptedTransactionIds, RpcBlock,
    RpcBlockVerboseData, RpcError, RpcHash, RpcHeader, RpcHeaderVerbosity, RpcMempoolEntry, RpcMempoolEntryByAddress,
    RpcMergesetBlockAcceptanceData, RpcMergesetBlockAcceptanceDataVerbosity, RpcResult, RpcTransaction, RpcTransactionInput,
    RpcTransactionInputVerboseData, RpcTransactionInputVerboseDataVerbosity, RpcTransactionInputVerbosity, RpcTransactionOutput,
    RpcTransactionOutputVerboseData, RpcTransactionOutputVerboseDataVerbosity, RpcTransactionOutputVerbosity,
    RpcTransactionVerboseData, RpcTransactionVerboseDataVerbosity, RpcTransactionVerbosity, RpcUtxoEntry, RpcUtxoEntryVerboseData,
    RpcUtxoEntryVerboseDataVerbosity, RpcUtxoEntryVerbosity,
};
use kaspa_txscript::{extract_script_pub_key_address, script_class::ScriptClass};
use std::{collections::HashMap, fmt::Debug, sync::Arc};
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
        let target = Uint256::from_compact_target_bits(bits);
        self.config.max_difficulty_target_f64 / target.as_f64()
    }

    /// Converts a consensus [`Block`] into an [`RpcBlock`], optionally including transaction verbose data.
    ///
    /// _GO-KASPAD: PopulateBlockWithVerboseData_
    pub async fn get_block(
        &self,
        consensus: &ConsensusProxy,
        block: &Block,
        include_transactions: bool,
        include_transaction_verbose_data: bool,
    ) -> RpcResult<RpcBlock> {
        let hash = block.hash();
        let ghostdag_data = consensus.async_get_ghostdag_data(hash).await?;
        let block_status = consensus.async_get_block_status(hash).await.unwrap();
        let children = consensus.async_get_block_children(hash).await.unwrap_or_default();
        let is_chain_block = consensus.async_is_chain_block(hash).await?;
        let verbose_data = Some(RpcBlockVerboseData {
            hash,
            difficulty: self.get_difficulty_ratio(block.header.bits),
            selected_parent_hash: ghostdag_data.selected_parent,
            transaction_ids: block.transactions.iter().map(|x| x.id()).collect(),
            is_header_only: block_status.is_header_only(),
            blue_score: ghostdag_data.blue_score,
            children_hashes: children,
            merge_set_blues_hashes: ghostdag_data.mergeset_blues,
            merge_set_reds_hashes: ghostdag_data.mergeset_reds,
            is_chain_block,
        });

        let transactions = if include_transactions {
            block
                .transactions
                .iter()
                .map(|x| self.get_transaction(consensus, x, Some(&block.header), include_transaction_verbose_data))
                .collect::<Vec<_>>()
        } else {
            vec![]
        };

        Ok(RpcBlock { header: Some(block.header.as_ref().into()), transactions, verbose_data })
    }

    pub fn get_mempool_entry(&self, consensus: &ConsensusProxy, transaction: &MutableTransaction) -> RpcMempoolEntry {
        let is_orphan = !transaction.is_fully_populated();
        let rpc_transaction = self.get_transaction(consensus, &transaction.tx, None, true);
        RpcMempoolEntry::new(transaction.calculated_fee.unwrap_or_default(), rpc_transaction, is_orphan)
    }

    pub fn get_mempool_entries_by_address(
        &self,
        consensus: &ConsensusProxy,
        address: Address,
        owner_transactions: &OwnerTransactions,
        transactions: &HashMap<TransactionId, MutableTransaction>,
    ) -> RpcMempoolEntryByAddress {
        let sending = self.get_owner_entries(consensus, &owner_transactions.sending_txs, transactions);
        let receiving = self.get_owner_entries(consensus, &owner_transactions.receiving_txs, transactions);
        RpcMempoolEntryByAddress::new(address, sending, receiving)
    }

    pub fn get_owner_entries(
        &self,
        consensus: &ConsensusProxy,
        transaction_ids: &TransactionIdSet,
        transactions: &HashMap<TransactionId, MutableTransaction>,
    ) -> Vec<RpcMempoolEntry> {
        transaction_ids.iter().map(|x| self.get_mempool_entry(consensus, transactions.get(x).expect("transaction exists"))).collect()
    }

    /// Converts a consensus [`Transaction`] into an [`RpcTransaction`], optionally including verbose data.
    ///
    /// _GO-KASPAD: PopulateTransactionWithVerboseData
    pub fn get_transaction(
        &self,
        consensus: &ConsensusProxy,
        transaction: &Transaction,
        header: Option<&Header>,
        include_verbose_data: bool,
    ) -> RpcTransaction {
        if include_verbose_data {
            let verbose_data = Some(RpcTransactionVerboseData {
                transaction_id: Some(transaction.id()),
                hash: Some(hash(transaction, false)),
                compute_mass: Some(consensus.calculate_transaction_non_contextual_masses(transaction).compute_mass),
                block_hash: header.map(|x| x.hash),
                block_time: header.map(|x| x.timestamp),
            });
            RpcTransaction {
                version: Some(transaction.version),
                inputs: transaction.inputs.iter().map(|x| self.get_transaction_input(x)).collect(),
                outputs: transaction.outputs.iter().map(|x| self.get_transaction_output(x)).collect(),
                lock_time: Some(transaction.lock_time),
                subnetwork_id: Some(transaction.subnetwork_id.clone()),
                gas: Some(transaction.gas),
                payload: Some(transaction.payload.clone()),
                mass: Some(transaction.mass()),
                verbose_data,
            }
        } else {
            transaction.into()
        }
    }

    fn get_transaction_input(&self, input: &TransactionInput) -> RpcTransactionInput {
        input.into()
    }

    fn get_transaction_output(&self, output: &TransactionOutput) -> RpcTransactionOutput {
        let script_public_key_type = Some(ScriptClass::from_script(&output.script_public_key));
        let address = extract_script_pub_key_address(&output.script_public_key, self.config.prefix()).ok();
        let verbose_data = address
            .map(|address| RpcTransactionOutputVerboseData { script_public_key_type, script_public_key_address: Some(address) });
        RpcTransactionOutput { value: Some(output.value), script_public_key: Some(output.script_public_key.clone()), verbose_data }
    }

    pub async fn get_virtual_chain_accepted_transaction_ids(
        &self,
        consensus: &ConsensusProxy,
        chain_path: &ChainPath,
        merged_blocks_limit: Option<usize>,
    ) -> RpcResult<Vec<RpcAcceptedTransactionIds>> {
        let acceptance_data = consensus.async_get_blocks_acceptance_data(chain_path.added.clone(), merged_blocks_limit).await.unwrap();
        Ok(chain_path
            .added
            .iter()
            .zip(acceptance_data.iter())
            .map(|(hash, block_data)| RpcAcceptedTransactionIds {
                accepting_block_hash: hash.to_owned(),
                accepted_transaction_ids: block_data
                    .iter()
                    .flat_map(|x| x.accepted_transactions.iter().map(|tx| tx.transaction_id))
                    .collect(),
            })
            .collect())
    }

    async fn get_header_with_verbosity(
        &self,
        consensus: &ConsensusProxy,
        verbosity: &RpcHeaderVerbosity,
        block_hash: RpcHash,
    ) -> RpcResult<RpcHeader> {
        let header = consensus.async_get_header(block_hash).await?;
        Ok(RpcHeader {
            hash: if verbosity.include_hash.unwrap_or(false) { Some(block_hash) } else { Default::default() },
            version: if verbosity.include_version.unwrap_or(false) { Some(header.version) } else { Default::default() },
            parents_by_level: if verbosity.include_parents_by_level.unwrap_or(false) {
                header.parents_by_level.to_owned()
            } else {
                Default::default()
            },
            hash_merkle_root: if verbosity.include_hash_merkle_root.unwrap_or(false) {
                Some(header.hash_merkle_root)
            } else {
                Default::default()
            },
            accepted_id_merkle_root: if verbosity.include_accepted_id_merkle_root.unwrap_or(false) {
                Some(header.accepted_id_merkle_root)
            } else {
                Default::default()
            },
            utxo_commitment: if verbosity.include_utxo_commitment.unwrap_or(false) {
                Some(header.utxo_commitment)
            } else {
                Default::default()
            },
            timestamp: if verbosity.include_timestamp.unwrap_or(false) { Some(header.timestamp) } else { Default::default() },
            bits: if verbosity.include_bits.unwrap_or(false) { Some(header.bits) } else { Default::default() },
            nonce: if verbosity.include_nonce.unwrap_or(false) { Some(header.nonce) } else { Default::default() },
            daa_score: if verbosity.include_daa_score.unwrap_or(false) { Some(header.daa_score) } else { Default::default() },
            blue_work: if verbosity.include_blue_work.unwrap_or(false) { Some(header.blue_work) } else { Default::default() },
            blue_score: if verbosity.include_blue_score.unwrap_or(false) { Some(header.blue_score) } else { Default::default() },
            pruning_point: if verbosity.include_pruning_point.unwrap_or(false) {
                Some(header.pruning_point)
            } else {
                Default::default()
            },
        })
    }

    fn convert_utxo_entry_with_verbosity(&self, utxo: UtxoEntry, verbosity: &RpcUtxoEntryVerbosity) -> RpcResult<RpcUtxoEntry> {
        Ok(RpcUtxoEntry {
            amount: if verbosity.include_amount.unwrap_or(false) { Some(utxo.amount) } else { Default::default() },
            script_public_key: if verbosity.include_script_public_key.unwrap_or(false) {
                Some(utxo.script_public_key.clone())
            } else {
                Default::default()
            },
            block_daa_score: if verbosity.include_block_daa_score.unwrap_or(false) {
                Some(utxo.block_daa_score)
            } else {
                Default::default()
            },
            is_coinbase: if verbosity.include_is_coinbase.unwrap_or(false) { Some(utxo.is_coinbase) } else { Default::default() },
            verbose_data: if let Some(utxo_entry_verbosity) = verbosity.verbose_data_verbosity.as_ref() {
                Some(self.get_utxo_verbose_data_with_verbosity(&utxo, utxo_entry_verbosity)?)
            } else {
                Default::default()
            },
        })
    }

    fn get_utxo_verbose_data_with_verbosity(
        &self,
        utxo: &UtxoEntry,
        verbosity: &RpcUtxoEntryVerboseDataVerbosity,
    ) -> RpcResult<RpcUtxoEntryVerboseData> {
        Ok(RpcUtxoEntryVerboseData {
            script_public_key_type: if verbosity.include_script_public_key_type.unwrap_or(false) {
                Some(ScriptClass::from_script(&utxo.script_public_key))
            } else {
                Default::default()
            },
            script_public_key_address: if verbosity.include_script_public_key_address.unwrap_or(false) {
                Some(
                    extract_script_pub_key_address(&utxo.script_public_key, self.config.prefix())
                        .map_err(|_| AddressError::InvalidAddress)?,
                )
            } else {
                Default::default()
            },
        })
    }

    fn get_input_verbose_data_with_verbosity(
        &self,
        utxo: Option<UtxoEntry>,
        verbosity: &RpcTransactionInputVerboseDataVerbosity,
    ) -> RpcResult<RpcTransactionInputVerboseData> {
        Ok(RpcTransactionInputVerboseData {
            utxo_entry: if let Some(utxo_entry_verbosity) = verbosity.utxo_entry_verbosity.as_ref() {
                if let Some(utxo) = utxo {
                    Some(self.convert_utxo_entry_with_verbosity(utxo, utxo_entry_verbosity)?)
                } else {
                    return Err(RpcError::ConsensusConverterNotFound("UtxoEntry".to_string()));
                }
            } else {
                Default::default()
            },
        })
    }

    fn get_transaction_verbose_data_with_verbosity(
        &self,
        transaction: &Transaction,
        block_hash: Hash,
        block_time: u64,
        compute_mass: u64,
        verbosity: &RpcTransactionVerboseDataVerbosity,
    ) -> RpcResult<RpcTransactionVerboseData> {
        Ok(RpcTransactionVerboseData {
            transaction_id: if verbosity.include_transaction_id.unwrap_or(false) {
                Some(transaction.id())
            } else {
                Default::default()
            },
            hash: if verbosity.include_hash.unwrap_or(false) { Some(hash(transaction, true)) } else { Default::default() },
            compute_mass: if verbosity.include_compute_mass.unwrap_or(false) { Some(compute_mass) } else { Default::default() },
            block_hash: if verbosity.include_block_hash.unwrap_or(false) { Some(block_hash) } else { Default::default() },
            block_time: if verbosity.include_block_time.unwrap_or(false) { Some(block_time) } else { Default::default() },
        })
    }

    fn get_transaction_output_verbose_data_with_verbosity(
        &self,
        output: &TransactionOutput,
        verbosity: &RpcTransactionOutputVerboseDataVerbosity,
    ) -> RpcResult<RpcTransactionOutputVerboseData> {
        Ok(RpcTransactionOutputVerboseData {
            script_public_key_type: if verbosity.include_script_public_key_type.unwrap_or(false) {
                Some(ScriptClass::from_script(&output.script_public_key))
            } else {
                Default::default()
            },
            script_public_key_address: if verbosity.include_script_public_key_address.unwrap_or(false) {
                Some(
                    extract_script_pub_key_address(&output.script_public_key, self.config.prefix())
                        .map_err(|_| AddressError::InvalidAddress)?,
                )
            } else {
                None
            },
        })
    }

    fn convert_transaction_output_with_verbosity(
        &self,
        output: &TransactionOutput,
        verbosity: &RpcTransactionOutputVerbosity,
    ) -> RpcResult<RpcTransactionOutput> {
        Ok(RpcTransactionOutput {
            value: if verbosity.include_amount.unwrap_or(false) { Some(output.value) } else { Default::default() },
            script_public_key: if verbosity.include_script_public_key.unwrap_or(false) {
                Some(output.script_public_key.clone())
            } else {
                Default::default()
            },
            verbose_data: if let Some(output_verbose_data_verbosity) = verbosity.verbose_data_verbosity.as_ref() {
                Some(self.get_transaction_output_verbose_data_with_verbosity(output, output_verbose_data_verbosity)?)
            } else {
                Default::default()
            },
        })
    }

    pub fn get_transaction_input_with_verbosity(
        &self,
        input: &TransactionInput,
        utxo: Option<UtxoEntry>,
        verbosity: &RpcTransactionInputVerbosity,
    ) -> RpcResult<RpcTransactionInput> {
        Ok(RpcTransactionInput {
            previous_outpoint: if verbosity.include_previous_outpoint.unwrap_or(false) {
                Some(input.previous_outpoint.into())
            } else {
                Default::default()
            },
            signature_script: if verbosity.include_signature_script.unwrap_or(false) {
                Some(input.signature_script.clone())
            } else {
                Default::default()
            },
            sequence: if verbosity.include_sequence.unwrap_or(false) { Some(input.sequence) } else { Default::default() },
            sig_op_count: if verbosity.include_sig_op_count.unwrap_or(false) { Some(input.sig_op_count) } else { Default::default() },
            verbose_data: if let Some(input_verbose_data_verbosity) = verbosity.verbose_data_verbosity.as_ref() {
                Some(self.get_input_verbose_data_with_verbosity(utxo, input_verbose_data_verbosity)?)
            } else {
                Default::default()
            },
        })
    }

    pub async fn convert_transaction_with_verbosity(
        &self,
        consensus: &ConsensusProxy,
        transaction: &Transaction,
        block_hash: Option<Hash>,
        block_time: Option<u64>,
        verbosity: &RpcTransactionVerbosity,
    ) -> RpcResult<RpcTransaction> {
        Ok(RpcTransaction {
            version: if verbosity.include_version.unwrap_or(false) { Some(transaction.version) } else { Default::default() },
            inputs: if let Some(ref input_verbosity) = verbosity.input_verbosity {
                transaction
                    .inputs
                    .iter()
                    .map(|x| self.get_transaction_input_with_verbosity(x, None, input_verbosity))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                Default::default()
            },
            outputs: if let Some(ref output_verbosity) = verbosity.output_verbosity {
                transaction
                    .outputs
                    .iter()
                    .map(|x| self.convert_transaction_output_with_verbosity(x, output_verbosity))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                Default::default()
            },
            lock_time: if verbosity.include_lock_time.unwrap_or(false) { Some(transaction.lock_time) } else { Default::default() },
            subnetwork_id: if verbosity.include_subnetwork_id.unwrap_or(false) {
                Some(transaction.subnetwork_id.clone())
            } else {
                Default::default()
            },
            gas: if verbosity.include_gas.unwrap_or(false) { Some(transaction.gas) } else { Default::default() },
            payload: if verbosity.include_payload.unwrap_or(false) { Some(transaction.payload.clone()) } else { Default::default() },
            mass: if verbosity.include_mass.unwrap_or(false) { Some(transaction.mass()) } else { Default::default() },
            verbose_data: if let Some(verbose_data_verbosity) = verbosity.verbose_data_verbosity.as_ref() {
                let block_time = if let Some(block_time) = block_time {
                    block_time
                } else {
                    consensus.async_get_header(block_hash.unwrap()).await?.timestamp
                };

                Some(self.get_transaction_verbose_data_with_verbosity(
                    transaction,
                    block_hash.unwrap(),
                    block_time,
                    consensus.calculate_transaction_non_contextual_masses(transaction).compute_mass,
                    verbose_data_verbosity,
                )?)
            } else {
                Default::default()
            },
        })
    }

    pub async fn convert_signable_transaction_with_verbosity(
        &self,
        consensus: &ConsensusProxy,
        transaction: &SignableTransaction,
        block_hash: Option<Hash>,
        block_time: Option<u64>,
        verbosity: &RpcTransactionVerbosity,
    ) -> RpcResult<RpcTransaction> {
        Ok(RpcTransaction {
            version: if verbosity.include_version.unwrap_or(false) { Some(transaction.tx.version) } else { Default::default() },
            inputs: if let Some(input_verbosity) = verbosity.input_verbosity.as_ref() {
                transaction
                    .tx
                    .inputs
                    .iter()
                    .enumerate()
                    .map(|(i, x)| self.get_transaction_input_with_verbosity(x, transaction.entries[i].clone(), input_verbosity))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                Default::default()
            },
            outputs: if let Some(output_verbosity) = verbosity.output_verbosity.as_ref() {
                transaction
                    .tx
                    .outputs
                    .iter()
                    .map(|x| self.convert_transaction_output_with_verbosity(x, output_verbosity))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                Default::default()
            },
            lock_time: if verbosity.include_lock_time.unwrap_or(false) { Some(transaction.tx.lock_time) } else { Default::default() },
            subnetwork_id: Some(transaction.tx.subnetwork_id.clone()),
            gas: Some(transaction.tx.gas),
            payload: Some(transaction.tx.payload.clone()),
            mass: Some(transaction.tx.mass()),
            verbose_data: if let Some(verbose_data_verbosity) = verbosity.verbose_data_verbosity.as_ref() {
                let block_time = if let Some(block_time) = block_time {
                    block_time
                } else if let Some(block_hash) = block_hash {
                    consensus.async_get_header(block_hash).await?.timestamp
                } else {
                    return Err(RpcError::ConsensusConverterNotFound("Block time not provided".to_string()));
                };

                Some(
                    self.get_transaction_verbose_data_with_verbosity(
                        &transaction.tx,
                        block_hash.unwrap(),
                        block_time,
                        transaction
                            .calculated_non_contextual_masses
                            .unwrap_or(consensus.calculate_transaction_non_contextual_masses(transaction.tx.as_ref()))
                            .compute_mass,
                        verbose_data_verbosity,
                    )?,
                )
            } else {
                Default::default()
            },
        })
    }

    pub async fn get_accepted_transactions_with_verbosity(
        &self,
        consensus: &ConsensusProxy,
        accepting_block_hash: RpcHash,
        tx_ids: Option<Vec<TransactionId>>,
        mergeset_block_acceptance: &MergesetBlockAcceptanceData,
        block_time: Option<u64>,
        verbosity: &RpcTransactionVerbosity,
    ) -> RpcResult<Vec<RpcTransaction>> {
        let txs = consensus
            .async_get_transactions_by_accepting_block(
                accepting_block_hash,
                tx_ids,
                if verbosity.requires_populated_transaction() {
                    TransactionType::SignableTransaction
                } else {
                    TransactionType::Transaction
                },
            )
            .await?;

        Ok(match txs {
            TransactionQueryResult::Transaction(txs) => {
                let mut converted = Vec::with_capacity(txs.len());

                for tx in txs.iter() {
                    converted.push({
                        let rpc_tx = self
                            .convert_transaction_with_verbosity(
                                consensus,
                                tx,
                                Some(mergeset_block_acceptance.block_hash),
                                block_time,
                                verbosity,
                            )
                            .await?;

                        if rpc_tx.is_empty() {
                            continue;
                        };

                        rpc_tx
                    });
                }

                converted
            }
            TransactionQueryResult::SignableTransaction(txs) => {
                let mut converted = Vec::with_capacity(txs.len());

                for tx in txs.iter() {
                    converted.push({
                        let rpc_tx = self
                            .convert_signable_transaction_with_verbosity(
                                consensus,
                                tx,
                                Some(mergeset_block_acceptance.block_hash),
                                block_time,
                                verbosity,
                            )
                            .await?;

                        if rpc_tx.is_empty() {
                            continue;
                        };

                        rpc_tx
                    });
                }

                converted
            }
        })
    }

    async fn get_mergeset_block_acceptance_data_with_verbosity(
        &self,
        consensus: &ConsensusProxy,
        accepting_chain_block: Hash,
        mergeset_block_acceptance: &MergesetBlockAcceptanceData,
        verbosity: &RpcMergesetBlockAcceptanceDataVerbosity,
    ) -> RpcResult<RpcMergesetBlockAcceptanceData> {
        let merged_header = if let Some(merged_header_verbosity) = verbosity.merged_header_verbosity.as_ref() {
            let merged_header =
                self.get_header_with_verbosity(consensus, merged_header_verbosity, mergeset_block_acceptance.block_hash).await?;
            if merged_header.is_empty() {
                Default::default()
            } else {
                Some(merged_header)
            }
        } else {
            Default::default()
        };

        let accepted_txs = if let Some(accepted_transaction_verbosity) = verbosity.accepted_transactions_verbosity.as_ref() {
            self.get_accepted_transactions_with_verbosity(
                consensus,
                accepting_chain_block,
                None,
                mergeset_block_acceptance,
                merged_header.as_ref().and_then(|x| x.timestamp.as_ref().map(|ts| *ts)),
                accepted_transaction_verbosity,
            )
            .await?
        } else {
            Vec::new()
        };

        Ok(RpcMergesetBlockAcceptanceData { merged_header, accepted_transactions: accepted_txs })
    }

    pub async fn get_acceptance_data_with_verbosity(
        &self,
        consensus: &ConsensusProxy,
        verbosity: &RpcAcceptanceDataVerbosity,
        chain_path: &ChainPath,
        merged_blocks_limit: Option<usize>,
    ) -> RpcResult<Vec<RpcAcceptanceData>> {
        if verbosity.accepting_chain_header_verbosity.is_none() && verbosity.mergeset_block_acceptance_data_verbosity.is_none() {
            // Early exit condition
            return Ok(Vec::new());
        }

        let acceptance_data = consensus.async_get_blocks_acceptance_data(chain_path.added.clone(), merged_blocks_limit).await.unwrap();
        let mut rpc_acceptance_data = Vec::<RpcAcceptanceData>::with_capacity(acceptance_data.len());

        for (accepting_chain_hash, acceptance_data) in chain_path.added.iter().zip(acceptance_data.iter()) {
            let accepting_chain_header = if let Some(verbosity) = verbosity.accepting_chain_header_verbosity.as_ref() {
                let header = self.get_header_with_verbosity(consensus, verbosity, *accepting_chain_hash).await?;
                if header.is_empty() {
                    Default::default()
                } else {
                    Some(header)
                }
            } else {
                Default::default()
            };

            if let Some(mergeset_block_acceptance_data_verbosity) = verbosity.mergeset_block_acceptance_data_verbosity.as_ref() {
                let mut rpc_mergeset_block_acceptance_data = Vec::with_capacity(acceptance_data.len());

                for mergeset_block_acceptance in acceptance_data.iter() {
                    rpc_mergeset_block_acceptance_data.push(
                        self.get_mergeset_block_acceptance_data_with_verbosity(
                            consensus,
                            *accepting_chain_hash,
                            mergeset_block_acceptance,
                            mergeset_block_acceptance_data_verbosity,
                        )
                        .await?,
                    );
                }

                rpc_acceptance_data.push(RpcAcceptanceData {
                    accepting_chain_header,
                    mergeset_block_acceptance_data: rpc_mergeset_block_acceptance_data,
                });
            } else {
                rpc_acceptance_data
                    .push(RpcAcceptanceData { accepting_chain_header, mergeset_block_acceptance_data: Default::default() });
            };
        }
        Ok(rpc_acceptance_data)
    }
}

#[async_trait]
impl Converter for ConsensusConverter {
    type Incoming = ConsensusNotification;
    type Outgoing = Notification;

    async fn convert(&self, incoming: ConsensusNotification) -> Notification {
        match incoming {
            consensus_notify::Notification::BlockAdded(msg) => {
                let session = self.consensus_manager.consensus().unguarded_session();
                // If get_block fails, rely on the infallible From implementation which will lack verbose data
                let block = Arc::new(self.get_block(&session, &msg.block, true, true).await.unwrap_or_else(|_| (&msg.block).into()));
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
