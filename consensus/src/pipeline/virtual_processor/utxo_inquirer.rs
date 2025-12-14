use std::{cmp, sync::Arc};

use kaspa_consensus_core::{
    acceptance_data::AcceptanceData,
    tx::{SignableTransaction, Transaction, UtxoEntry},
    utxo::{utxo_diff::ImmutableUtxoDiff, utxo_inquirer::UtxoInquirerError},
};
use kaspa_core::{trace, warn};
use kaspa_hashes::Hash;

use crate::model::stores::{
    acceptance_data::AcceptanceDataStoreReader, block_transactions::BlockTransactionsStoreReader, headers::HeaderStoreReader,
    selected_chain::SelectedChainStoreReader, utxo_diffs::UtxoDiffsStoreReader,
};

use super::VirtualStateProcessor;

impl VirtualStateProcessor {
    /// Returns the fully populated transaction with the given txid which was accepted at the provided accepting_block_daa_score.
    /// The argument `accepting_block_daa_score` is expected to be the DAA score of the accepting chain block of `txid`.
    ///
    /// *Assumed to be called under the pruning read lock.*
    pub fn get_populated_transaction(
        &self,
        txid: Hash,
        accepting_block_daa_score: u64,
        retention_period_root_hash: Hash,
    ) -> Result<SignableTransaction, UtxoInquirerError> {
        let retention_period_root_daa_score = self
            .headers_store
            .get_daa_score(retention_period_root_hash)
            .map_err(|_| UtxoInquirerError::MissingCompactHeaderForBlockHash(retention_period_root_hash))?;

        if accepting_block_daa_score < retention_period_root_daa_score {
            // Early exit if target daa score is lower than that of pruning point's daa score:
            return Err(UtxoInquirerError::AlreadyPruned);
        }

        let (matching_chain_block_hash, acceptance_data) =
            self.find_accepting_chain_block_hash_at_daa_score(accepting_block_daa_score, retention_period_root_hash)?;

        // Expected to never fail, since we found the acceptance data and therefore there must be matching diff
        let utxo_diff = self
            .utxo_diffs_store
            .get(matching_chain_block_hash)
            .map_err(|_| UtxoInquirerError::MissingUtxoDiffForChainBlock(matching_chain_block_hash))?;

        let tx = self.find_tx_from_acceptance_data(txid, &acceptance_data)?;

        let mut populated_tx = SignableTransaction::new(tx);

        let removed_diffs = utxo_diff.removed();

        populated_tx.tx.inputs.iter().enumerate().for_each(|(index, input)| {
            let filled_utxo = if let Some(utxo_entry) = removed_diffs.get(&input.previous_outpoint) {
                Some(utxo_entry.clone().to_owned())
            } else {
                // This handles this rare scenario:
                // - UTXO0 is spent by TX1 and creates UTXO1
                // - UTXO1 is spent by TX2 and creates UTXO2
                // - A chain block happens to accept both of these
                // In this case, removed_diff wouldn't contain the outpoint of the created-and-immediately-spent UTXO
                // so we use the transaction (which also has acceptance data in this block) and look at its outputs
                let other_txid = input.previous_outpoint.transaction_id;
                let other_tx = self.find_tx_from_acceptance_data(other_txid, &acceptance_data).unwrap();
                let output = &other_tx.outputs[input.previous_outpoint.index as usize];
                let utxo_entry =
                    UtxoEntry::new(output.value, output.script_public_key.clone(), accepting_block_daa_score, other_tx.is_coinbase());
                Some(utxo_entry)
            };

            populated_tx.entries[index] = filled_utxo;
        });

        Ok(populated_tx)
    }

    /// Find the accepting chain block hash at the given DAA score by binary searching
    /// through selected chain store using indexes.
    /// This method assumes that local caller have acquired the pruning read lock to guarantee
    /// consistency between reads on the selected_chain_store and headers_store (as well as
    /// other stores outside). If no such lock is acquired, this method tries to find
    /// the accepting chain block hash on a best effort basis (may fail if parts of the data
    /// are pruned between two sequential calls)
    fn find_accepting_chain_block_hash_at_daa_score(
        &self,
        target_daa_score: u64,
        retention_period_root_hash: Hash,
    ) -> Result<(Hash, Arc<AcceptanceData>), UtxoInquirerError> {
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

        let acceptance_data = self
            .acceptance_data_store
            .get(matching_chain_block_hash)
            .map_err(|_| UtxoInquirerError::MissingAcceptanceDataForChainBlock(matching_chain_block_hash))?;

        Ok((matching_chain_block_hash, acceptance_data))
    }

    /// Finds a transaction's containing block hash and index within block through
    /// the accepting block acceptance data
    fn find_containing_block_and_index_from_acceptance_data(
        &self,
        txid: Hash,
        acceptance_data: &AcceptanceData,
    ) -> Option<(Hash, usize)> {
        acceptance_data.iter().find_map(|mbad| {
            let tx_arr_index =
                mbad.accepted_transactions.iter().find_map(|tx| (tx.transaction_id == txid).then_some(tx.index_within_block as usize));
            tx_arr_index.map(|index| (mbad.block_hash, index))
        })
    }

    /// Finds a transaction through the accepting block acceptance data (and using indexed info therein for
    /// finding the tx in the block transactions store)
    fn find_tx_from_acceptance_data(&self, txid: Hash, acceptance_data: &AcceptanceData) -> Result<Transaction, UtxoInquirerError> {
        let (containing_block, index) = self
            .find_containing_block_and_index_from_acceptance_data(txid, acceptance_data)
            .ok_or(UtxoInquirerError::MissingContainingAcceptanceForTx(txid))?;

        let tx = self
            .block_transactions_store
            .get(containing_block)
            .map_err(|_| UtxoInquirerError::MissingBlockFromBlockTxStore(containing_block))
            .and_then(|block_txs| {
                block_txs.get(index).cloned().ok_or(UtxoInquirerError::MissingTransactionIndexOfBlock(index, containing_block))
            })?;

        if tx.id() != txid {
            // Should never happen, but do a sanity check. This would mean something went wrong with storing block transactions.
            // Sanity check is necessary to guarantee that this function will never give back a wrong address (err on the side of not found)
            warn!("Expected {} to match {} when checking block_transaction_store using array index of transaction", tx.id(), txid);
            return Err(UtxoInquirerError::UnexpectedTransactionMismatch(tx.id(), txid));
        }

        Ok(tx)
    }
}
