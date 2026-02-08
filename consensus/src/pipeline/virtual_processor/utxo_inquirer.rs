use std::{cmp, collections::HashSet, sync::Arc};

use kaspa_consensus_core::{
    acceptance_data::{AcceptanceData, MergesetBlockAcceptanceData},
    tx::{SignableTransaction, Transaction, TransactionId, TransactionIndexType, TransactionOutpoint, UtxoEntry},
    utxo::{
        utxo_diff::ImmutableUtxoDiff,
        utxo_inquirer::{UtxoInquirerError, UtxoInquirerFindTxsFromAcceptanceDataError, UtxoInquirerResult},
    },
};
use kaspa_core::trace;
use kaspa_hashes::Hash;

use crate::model::{
    services::reachability::ReachabilityService,
    stores::{
        acceptance_data::AcceptanceDataStoreReader, block_transactions::BlockTransactionsStoreReader, headers::HeaderStoreReader,
        selected_chain::SelectedChainStoreReader, utxo_diffs::UtxoDiffsStoreReader, utxo_set::UtxoSetStoreReader,
        virtual_state::VirtualStateStoreReader,
    },
};

use super::VirtualStateProcessor;

pub struct MergesetAcceptanceMetaData {
    pub accepting_block_hash: Hash,
    pub acceptance_data: Arc<AcceptanceData>,
    pub accepting_daa_score: u64,
    pub mergeset_idx: usize,
}

impl VirtualStateProcessor {
    pub fn find_accepting_data(
        &self,
        block_hash: Hash,
        retention_period_root_hash: Hash,
        sink_hash: Hash,
    ) -> UtxoInquirerResult<Option<MergesetAcceptanceMetaData>> {
        // accepting block hash, daa score, acceptance data
        // check if block is an ancestor of the sink block, i.e. we expect it to be accepted
        if self.reachability_service.is_dag_ancestor_of(block_hash, sink_hash) {
            // find the first "possible" accepting chain block
            let ancestor = self.find_accepting_chain_block_hash_at_daa_score(
                self.headers_store
                    .get_daa_score(block_hash)
                    .map_err(|_| UtxoInquirerError::MissingCompactHeaderForBlockHash(block_hash))?,
                retention_period_root_hash,
            )?;
            // iterate forward from the ancestor to the sink block, looking for the accepting block
            for candidate in self.reachability_service.forward_chain_iterator(ancestor, sink_hash, true) {
                let acceptance_data = self
                    .acceptance_data_store
                    .get(candidate)
                    .map_err(|_| UtxoInquirerError::MissingAcceptanceDataForChainBlock(candidate))?;
                for (i, mbad) in acceptance_data.iter().enumerate() {
                    if mbad.block_hash == block_hash {
                        return Ok(Some(MergesetAcceptanceMetaData {
                            accepting_block_hash: candidate,
                            acceptance_data,
                            accepting_daa_score: self
                                .headers_store
                                .get_daa_score(candidate)
                                .map_err(|_| UtxoInquirerError::MissingCompactHeaderForBlockHash(candidate))?,
                            mergeset_idx: i,
                        }));
                    }
                }
            }
        }
        Ok(None)
    }

    pub fn populate_block_transactions(
        &self,
        block_hash: Hash,
        txs: Vec<Transaction>,
        retention_period_root_hash: Hash,
    ) -> UtxoInquirerResult<Vec<SignableTransaction>> {
        let virual_state_read = self.virtual_stores.read();
        let sink_hash = virual_state_read.state.get().expect("expected virtual state").ghostdag_data.selected_parent;
        let utxo_store = &virual_state_read.utxo_set;

        let mut signable_transactions = Vec::with_capacity(txs.len());

        if let Some(mergeset_meta_data) = self.find_accepting_data(block_hash, retention_period_root_hash, sink_hash)? {
            // We have a mergeset acceptance, so we most factor in the acceptance data to populate the transactions
            let utxo_diff = self
                .utxo_diffs_store
                .get(mergeset_meta_data.accepting_block_hash)
                .map_err(|_| UtxoInquirerError::MissingUtxoDiffForChainBlock(mergeset_meta_data.accepting_block_hash))?;
            for tx in txs.into_iter() {
                let mut entries = Vec::with_capacity(tx.inputs.len());
                for input in tx.inputs.iter() {
                    if let Some(utxo) = utxo_diff.removed().get(&input.previous_outpoint) {
                        // first check: if it was accepted, i.e. removed in the diff
                        entries.push(utxo.clone());
                    } else if let Some(utxo) = utxo_store.get(&input.previous_outpoint).ok().map(|arc| (*arc).clone()) {
                        // secound check: if it was not accepted, it may be in the utxo set
                        entries.push(utxo);
                    } else {
                        // third check: if it was not accepted and not in the utxo set, it may have been created and spent in a parallel block.
                        entries.push(self.resolve_missing_outpoint(
                            &input.previous_outpoint,
                            &mergeset_meta_data.acceptance_data,
                            mergeset_meta_data.accepting_daa_score,
                        )?);
                    }
                }
                signable_transactions.push(SignableTransaction::with_entries(tx, entries));
            }
        } else {
            // We don't have a mergeset acceptance, so we use the utxo set solely to populate the transactions.
            // we do not expect to find the outpoints anywhere else.
            for tx in txs.into_iter() {
                let mut entries = Vec::with_capacity(tx.inputs.len());
                for input in tx.inputs.iter() {
                    match utxo_store.get(&input.previous_outpoint) {
                        Ok(utxo) => entries.push((*utxo).clone()),
                        Err(_) => return Err(UtxoInquirerError::MissingUtxoEntryForOutpoint(input.previous_outpoint)),
                    }
                }
                signable_transactions.push(SignableTransaction::with_entries(tx, entries));
            }
        }

        Ok(signable_transactions)
    }

    fn resolve_missing_outpoint(
        &self,
        outpoint: &TransactionOutpoint,
        acceptance_data: &AcceptanceData,
        accepting_block_daa_score: u64,
    ) -> UtxoInquirerResult<UtxoEntry> {
        // This handles this rare scenario:
        // - UTXO0 is spent by TX1 and creates UTXO1
        // - UTXO1 is spent by TX2 and creates UTXO2
        // - A chain block happens to accept both of these
        // In this case, removed_diff wouldn't contain the outpoint of the created-and-immediately-spent UTXO
        // so we use the transaction (which also has acceptance data in this block) and look at its outputs
        let other_tx = &self.find_txs_from_acceptance_data(Some(vec![outpoint.transaction_id]), acceptance_data)?[0];
        let output = &other_tx.outputs[outpoint.index as usize];
        let utxo_entry =
            UtxoEntry::new(output.value, output.script_public_key.clone(), accepting_block_daa_score, other_tx.is_coinbase());
        Ok(utxo_entry)
    }

    pub fn get_populated_transactions_by_block_acceptance_data(
        &self,
        tx_ids: Option<Vec<TransactionId>>,
        block_acceptance_data: MergesetBlockAcceptanceData,
        accepting_block: Hash,
    ) -> UtxoInquirerResult<Vec<SignableTransaction>> {
        let accepting_daa_score = self
            .headers_store
            .get_daa_score(accepting_block)
            .map_err(|_| UtxoInquirerError::MissingCompactHeaderForBlockHash(accepting_block))?;

        let utxo_diff = self
            .utxo_diffs_store
            .get(accepting_block)
            .map_err(|_| UtxoInquirerError::MissingUtxoDiffForChainBlock(accepting_block))?;

        let acceptance_data_for_this_block = vec![block_acceptance_data];

        let txs = self.find_txs_from_acceptance_data(tx_ids, &acceptance_data_for_this_block)?;

        let mut populated_txs = Vec::<SignableTransaction>::with_capacity(txs.len());

        for tx in txs.iter() {
            let mut entries = Vec::with_capacity(tx.inputs.len());
            for input in tx.inputs.iter() {
                let filled_utxo = if let Some(utxo) = utxo_diff.removed().get(&input.previous_outpoint).cloned() {
                    Some(utxo)
                } else if let Some(utxo) = populated_txs.iter().map(|ptx| &ptx.tx).chain(txs.iter()).find_map(|tx| {
                    if tx.id() == input.previous_outpoint.transaction_id {
                        let output = &tx.outputs[input.previous_outpoint.index as usize];
                        Some(UtxoEntry::new(output.value, output.script_public_key.clone(), accepting_daa_score, tx.is_coinbase()))
                    } else {
                        None
                    }
                }) {
                    Some(utxo)
                } else {
                    // When trying to resolve the missing outpoint, the transaction data we need is going to come from the acceptance
                    // data of some other block that was merged by this chain block. We cannot use "acceptance_data_for_this_block" as that
                    // definitely cannot contain the missing outpoint. A single block cannot accept interdependent txs, therefore the dependency tx
                    // must have been included by a different block.
                    // So we need to acquire the full acceptance data here of all the blocks merged and accepted by this chain block
                    // and pass that down to resolve_missing_outpoint.
                    let full_acceptance_data = self
                        .acceptance_data_store
                        .get(accepting_block)
                        .map_err(|_| UtxoInquirerError::MissingAcceptanceDataForChainBlock(accepting_block))?;
                    Some(self.resolve_missing_outpoint(&input.previous_outpoint, &full_acceptance_data, accepting_daa_score)?)
                };

                entries.push(filled_utxo.ok_or(UtxoInquirerError::MissingUtxoEntryForOutpoint(input.previous_outpoint))?);
            }
            populated_txs.push(SignableTransaction::with_entries(tx.clone(), entries));
        }

        Ok(populated_txs)
    }

    pub fn get_populated_transactions_by_accepting_block(
        &self,
        tx_ids: Option<Vec<TransactionId>>,
        accepting_block: Hash,
    ) -> UtxoInquirerResult<Vec<SignableTransaction>> {
        let acceptance_data = self
            .acceptance_data_store
            .get(accepting_block)
            .map_err(|_| UtxoInquirerError::MissingAcceptanceDataForChainBlock(accepting_block))?;

        let accepting_daa_score = self
            .headers_store
            .get_daa_score(accepting_block)
            .map_err(|_| UtxoInquirerError::MissingCompactHeaderForBlockHash(accepting_block))?;
        // Expected to never fail, since we found the acceptance data and therefore there must be matching diff
        let utxo_diff = self
            .utxo_diffs_store
            .get(accepting_block)
            .map_err(|_| UtxoInquirerError::MissingUtxoDiffForChainBlock(accepting_block))?;

        let txs = self.find_txs_from_acceptance_data(tx_ids, &acceptance_data)?;

        let mut populated_txs = Vec::<SignableTransaction>::with_capacity(txs.len());

        for tx in txs.iter() {
            let mut entries = Vec::with_capacity(tx.inputs.len());
            for input in tx.inputs.iter() {
                let filled_utxo = if let Some(utxo) = utxo_diff.removed().get(&input.previous_outpoint).cloned() {
                    Some(utxo)
                } else if let Some(utxo) = populated_txs.iter().map(|ptx| &ptx.tx).chain(txs.iter()).find_map(|tx| {
                    if tx.id() == input.previous_outpoint.transaction_id {
                        let output = &tx.outputs[input.previous_outpoint.index as usize];
                        Some(UtxoEntry::new(output.value, output.script_public_key.clone(), accepting_daa_score, tx.is_coinbase()))
                    } else {
                        None
                    }
                }) {
                    Some(utxo)
                } else {
                    Some(self.resolve_missing_outpoint(&input.previous_outpoint, &acceptance_data, accepting_daa_score)?)
                };

                entries.push(filled_utxo.ok_or(UtxoInquirerError::MissingUtxoEntryForOutpoint(input.previous_outpoint))?);
            }
            populated_txs.push(SignableTransaction::with_entries(tx.clone(), entries));
        }

        Ok(populated_txs)
    }

    /// Returns the fully populated transactions with the given tx ids which were accepted at the provided accepting_block_daa_score.
    /// The argument `accepting_block_daa_score` is expected to be the DAA score of the accepting chain block of `tx ids`.
    ///
    /// *Assumed to be called under the pruning read lock.*
    ///
    pub fn get_populated_transactions_by_accepting_daa_score(
        &self,
        tx_ids: Option<Vec<TransactionId>>,
        accepting_block_daa_score: u64,
        retention_period_root_hash: Hash,
    ) -> UtxoInquirerResult<Vec<SignableTransaction>> {
        let matching_chain_block_hash =
            self.find_accepting_chain_block_hash_at_daa_score(accepting_block_daa_score, retention_period_root_hash)?;

        self.get_populated_transactions_by_accepting_block(tx_ids, matching_chain_block_hash)
    }
    /// Find the accepting chain block hash at the given DAA score by binary searching
    /// through selected chain store using indexes.
    /// This method assumes that local caller have acquired the pruning read lock to guarantee
    /// consistency between reads on the selected_chain_store and headers_store (as well as
    /// other stores outside). If no such lock is acquired, this method tries to find
    /// the accepting chain block hash on a best effort basis (may fail if parts of the data
    /// are pruned between two sequential calls)
    pub fn find_accepting_chain_block_hash_at_daa_score(
        &self,
        target_daa_score: u64,
        retention_period_root_hash: Hash,
    ) -> UtxoInquirerResult<Hash> {
        let sc_read = self.selected_chain_store.read();

        let retention_period_root_index = sc_read
            .get_by_hash(retention_period_root_hash)
            .map_err(|_| UtxoInquirerError::MissingIndexForHash(retention_period_root_hash))?;
        let (tip_index, tip_hash) = sc_read.get_tip().map_err(|_| UtxoInquirerError::MissingTipData)?;
        let tip_daa_score =
            self.headers_store.get_daa_score(tip_hash).map_err(|_| UtxoInquirerError::MissingCompactHeaderForBlockHash(tip_hash))?;

        // For a chain segment it holds that len(segment) <= daa_score(segment end) - daa_score(segment start). This is true
        // because each chain block increases the daa score by at least one. Hence we can lower bound our search by high index
        // minus the daa score gap as done below
        let mut low_index = tip_index.saturating_sub(tip_daa_score.saturating_sub(target_daa_score)).max(retention_period_root_index);
        let mut high_index = tip_index;

        let matching_chain_block_hash = loop {
            // Binary search for the chain block that matches the target_daa_score
            // 0. Get the mid point index
            let mid = low_index + (high_index - low_index) / 2;

            // 1. Get the chain block hash at that index. Error if we cannot find a hash at that index
            let hash = sc_read.get_by_index(mid).map_err(|_| {
                trace!("Did not find a hash at index {}", mid);
                UtxoInquirerError::MissingHashAtIndex(mid)
            })?;

            // 2. Get the daa_score. Error if the header is not found
            let daa_score = self.headers_store.get_daa_score(hash).map_err(|_| {
                trace!("Did not find a header with hash {}", hash);
                UtxoInquirerError::MissingCompactHeaderForBlockHash(hash)
            })?;

            // 3. Compare block daa score to our target
            match daa_score.cmp(&target_daa_score) {
                cmp::Ordering::Equal => {
                    // We found the chain block we need
                    break hash;
                }
                cmp::Ordering::Greater => {
                    high_index = mid - 1;
                }
                cmp::Ordering::Less => {
                    low_index = mid + 1;
                }
            }

            if low_index > high_index {
                return Err(UtxoInquirerError::NoTxAtScore);
            }
        };

        Ok(matching_chain_block_hash)
    }

    /// Finds a transaction's containing block hash and index within block through
    /// the accepting block acceptance data
    fn _find_containing_blocks_and_indices_from_acceptance_data(
        &self,
        tx_ids: &[TransactionId],
        acceptance_data: &AcceptanceData,
    ) -> Vec<(Hash, Vec<TransactionIndexType>)> {
        let tx_set = tx_ids.iter().collect::<HashSet<_>>();
        let mut collected = 0usize;

        let mut result = Vec::with_capacity(acceptance_data.len());

        'outer: for mbad in acceptance_data.iter() {
            for atx in mbad.accepted_transactions.iter() {
                let mut indices = Vec::new();
                if tx_set.contains(&atx.transaction_id) {
                    indices.push(atx.index_within_block);
                    collected += 1;
                    if collected == tx_ids.len() {
                        result.push((mbad.block_hash, indices));
                        break 'outer;
                    }
                }
                if !indices.is_empty() {
                    result.push((mbad.block_hash, indices));
                }
            }
        }

        result
    }

    /// Finds transaction(s) through a provided accepting block acceptance data
    ///
    /// Arguments:
    /// * `tx_ids`: an optional list of tx id(s) to resolve. When passing `None`, the accepted transaction ids
    ///   contained in `acceptance_data` is used as a filter.
    ///   This default behavior ensures only the accepted transactions by this mergeset are resolved.
    /// * `acceptance_data`: accepting block acceptance data
    ///
    /// Limitations:
    /// * `tx_ids` currently only allow filtering with exactly one transaction, not multiple
    fn find_txs_from_acceptance_data(
        &self,
        tx_ids: Option<Vec<TransactionId>>,
        acceptance_data: &AcceptanceData,
    ) -> UtxoInquirerResult<Vec<Transaction>> {
        match tx_ids.as_deref() {
            None => {
                // no filter passed, using default accepted transactions by mergeset filter
                let total_accepted: usize = acceptance_data.iter().map(|mbad| mbad.accepted_transactions.len()).sum();

                // accepted transactions data of this mergeset
                let mut all_txs = Vec::with_capacity(total_accepted);

                for mbad in acceptance_data {
                    let block_txs = self
                        .block_transactions_store
                        .get(mbad.block_hash)
                        .map_err(|_| UtxoInquirerError::MissingBlockFromBlockTxStore(mbad.block_hash))?;

                    for accepted in &mbad.accepted_transactions {
                        let idx = accepted.index_within_block as usize;

                        let tx = block_txs.get(idx).ok_or(UtxoInquirerError::MissingTransactionIndexOfBlock(idx, mbad.block_hash))?;

                        all_txs.push(tx.clone());
                    }
                }
                Ok(all_txs)
            }
            Some([]) => {
                // empty filter -> error
                Err(UtxoInquirerFindTxsFromAcceptanceDataError::TxIdsFilterIsEmptyError.into())
            }
            Some([tx_id]) => {
                // single element filter, optimize for this case specifically
                let (containing_block, index) = acceptance_data
                    .iter()
                    .find_map(|mbad| {
                        let tx_arr_index = mbad
                            .accepted_transactions
                            .iter()
                            .find_map(|tx| (tx.transaction_id == *tx_id).then_some(tx.index_within_block as usize));
                        tx_arr_index.map(|index| (mbad.block_hash, index))
                    })
                    .ok_or_else(|| UtxoInquirerError::MissingQueriedTransactions(vec![*tx_id]))?;

                let tx = self
                    .block_transactions_store
                    .get(containing_block)
                    .map_err(|_| UtxoInquirerError::MissingBlockFromBlockTxStore(containing_block))
                    .and_then(|block_txs| {
                        block_txs.get(index).cloned().ok_or(UtxoInquirerError::MissingTransactionIndexOfBlock(index, containing_block))
                    })?;

                Ok(vec![tx])
            }
            Some(_more) => {
                Err(UtxoInquirerFindTxsFromAcceptanceDataError::TxIdsFilterNeedsLessOrEqualThanOneElementError.into())
                // TODO: currently there is no calling site that needs to make arbitrary filter by tx_ids with more than 1 element
                // But it should be considered a future enhancement to address
                // artifact implementation that has been commented, keeping it for track record as long as it's unimplemented
                /*

                let mut txs = HashMap::<TransactionId, Transaction, _>::new();
                for (containing_block, indices) in
                    self.find_containing_blocks_and_indices_from_acceptance_data(&tx_ids, acceptance_data)
                {
                    let mut indice_iter = indices.iter();
                    let mut target_index = (*indice_iter.next().unwrap()) as usize;
                    let cut_off_index = (*indices.last().unwrap()) as usize;

                    txs.extend(
                        self.block_transactions_store
                            .get(containing_block)
                            .map_err(|_| UtxoInquirerError::MissingBlockFromBlockTxStore(containing_block))?
                            .unwrap_or_clone()
                            .into_iter()
                            .enumerate()
                            .take_while(|(i, _)| *i <= cut_off_index)
                            .filter_map(|(i, tx)| {
                                if i == target_index {
                                    target_index = (*indice_iter.next().unwrap()) as usize;
                                    Some((tx.id(), tx))
                                } else {
                                    None
                                }
                            }),
                    );
                }

                /*
                if txs.len() < tx_ids.len() {
                    // The query includes txs which are not in the acceptance data, we constitute this as an error.
                    return Err(UtxoInquirerError::MissingQueriedTransactions(
                        tx_ids.iter().filter(|tx_id| !txs.contains_key(*tx_id)).copied().collect::<Vec<_>>(),
                    ));
                };
                */

                return Ok(tx_ids.iter().map(|tx_id| txs.remove(tx_id).expect("expected queried tx id")).collect::<Vec<_>>())
                                    */
            }
        }
    }
}
