use std::{cmp, sync::Arc};

use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::{
    acceptance_data::MergesetBlockAcceptanceData, return_address::ReturnAddressError, utxo::utxo_diff::ImmutableUtxoDiff,
};
use kaspa_core::{trace, warn};
use kaspa_hashes::Hash;
use kaspa_txscript::extract_script_pub_key_address;

use crate::model::stores::{
    acceptance_data::AcceptanceDataStoreReader, block_transactions::BlockTransactionsStoreReader, headers::HeaderStoreReader,
    selected_chain::SelectedChainStoreReader, utxo_diffs::UtxoDiffsStoreReader,
};

use super::VirtualStateProcessor;

impl VirtualStateProcessor {
    /// Returns the first paying address for `txid` (i.e., the address signed by its first input).
    /// The argument `target_daa_score` is expected to be the DAA score of the accepting chain block of `txid`.
    ///
    /// *Assumed to be called under the pruning read lock.*
    pub fn get_utxo_return_address(
        &self,
        txid: Hash,
        target_daa_score: u64,
        source_hash: Hash,
        prefix: Prefix,
    ) -> Result<Address, ReturnAddressError> {
        let source_daa_score = self
            .headers_store
            .get_compact_header_data(source_hash)
            .map(|compact_header| compact_header.daa_score)
            .map_err(|_| ReturnAddressError::MissingCompactHeaderForBlockHash(source_hash))?;

        if target_daa_score < source_daa_score {
            // Early exit if target daa score is lower than that of pruning point's daa score:
            return Err(ReturnAddressError::AlreadyPruned);
        }

        let (matching_chain_block_hash, acceptance_data) =
            self.find_accepting_chain_block_hash_at_daa_score(target_daa_score, source_hash)?;

        let (containing_block, index) = self
            .find_containing_block_and_index_from_acceptance_data(txid, acceptance_data.clone())
            .ok_or(ReturnAddressError::MissingContainingAcceptanceForTx(txid))?;

        // Found Merged block containing the TXID
        let tx = self
            .block_transactions_store
            .get(containing_block)
            .map_err(|_| ReturnAddressError::MissingBlockFromBlockTxStore(containing_block))
            .and_then(|block_txs| {
                block_txs.get(index).cloned().ok_or(ReturnAddressError::MissingTransactionIndexOfBlock(index, containing_block))
            })?;

        if tx.id() != txid {
            // Should never happen, but do a sanity check. This would mean something went wrong with storing block transactions
            // Sanity check is necessary to guarantee that this function will never give back a wrong address (err on the side of not found)
            warn!("Expected {} to match {} when checking block_transaction_store using array index of transaction", tx.id(), txid);
            return Err(ReturnAddressError::UnexpectedTransactionMismatch(tx.id(), txid));
        }

        if tx.inputs.is_empty() {
            // A transaction may have no inputs (like a coinbase transaction)
            return Err(ReturnAddressError::TxFromCoinbase);
        }

        let first_input_prev_outpoint = &tx.inputs[0].previous_outpoint;
        // Expected to never fail, since we found the acceptance data and therefore there must be matching diff
        let utxo_diff = self
            .utxo_diffs_store
            .get(matching_chain_block_hash)
            .map_err(|_| ReturnAddressError::MissingUtxoDiffForChainBlock(matching_chain_block_hash))?;
        let removed_diffs = utxo_diff.removed();

        let spk = if let Some(utxo_entry) = removed_diffs.get(first_input_prev_outpoint) {
            utxo_entry.script_public_key.clone()
        } else {
            // This handles this rare scenario:
            // - UTXO0 is spent by TX1 and creates UTXO1
            // - UTXO1 is spent by TX2 and creates UTXO2
            // - A chain block happens to accept both of these
            // In this case, removed_diff wouldn't contain the outpoint of the created-and-immediately-spent UTXO
            // so we use the transaction (which also has acceptance data in this block) and look at its outputs
            let other_txid = first_input_prev_outpoint.transaction_id;
            let (other_containing_block, other_index) = self
                .find_containing_block_and_index_from_acceptance_data(other_txid, acceptance_data)
                .ok_or(ReturnAddressError::MissingOtherTransactionAcceptanceData(other_txid))?;
            let other_tx = self
                .block_transactions_store
                .get(other_containing_block)
                .map_err(|_| ReturnAddressError::MissingBlockFromBlockTxStore(other_containing_block))
                .and_then(|block_txs| {
                    block_txs
                        .get(other_index)
                        .cloned()
                        .ok_or(ReturnAddressError::MissingTransactionIndexOfBlock(other_index, other_containing_block))
                })?;

            other_tx.outputs[first_input_prev_outpoint.index as usize].script_public_key.clone()
        };

        if let Ok(address) = extract_script_pub_key_address(&spk, prefix) {
            Ok(address)
        } else {
            Err(ReturnAddressError::NonStandard)
        }
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
        source_hash: Hash,
    ) -> Result<(Hash, Arc<Vec<MergesetBlockAcceptanceData>>), ReturnAddressError> {
        let sc_read = self.selected_chain_store.read();

        let source_index = sc_read.get_by_hash(source_hash).map_err(|_| ReturnAddressError::MissingIndexForHash(source_hash))?;
        let (tip_index, tip_hash) = sc_read.get_tip().map_err(|_| ReturnAddressError::MissingTipData)?;
        let tip_daa_score = self
            .headers_store
            .get_compact_header_data(tip_hash)
            .map(|tip| tip.daa_score)
            .map_err(|_| ReturnAddressError::MissingCompactHeaderForBlockHash(tip_hash))?;

        // For a chain segment it holds that len(segment) <= daa_score(segment end) - daa_score(segment start). This is true
        // because each chain block increases the daa score by at least one. Hence we can lower bound our search by high index
        // minus the daa score gap as done below
        let mut low_index = tip_index.saturating_sub(tip_daa_score.saturating_sub(target_daa_score)).max(source_index);
        let mut high_index = tip_index;

        let matching_chain_block_hash = loop {
            // Binary search for the chain block that matches the target_daa_score
            // 0. Get the mid point index
            let mid = low_index + (high_index - low_index) / 2;

            // 1. Get the chain block hash at that index. Error if we cannot find a hash at that index
            let hash = sc_read.get_by_index(mid).map_err(|_| {
                trace!("Did not find a hash at index {}", mid);
                ReturnAddressError::MissingHashAtIndex(mid)
            })?;

            // 2. Get the compact header so we have access to the daa_score. Error if we cannot find the header
            let compact_header = self.headers_store.get_compact_header_data(hash).map_err(|_| {
                trace!("Did not find a compact header with hash {}", hash);
                ReturnAddressError::MissingCompactHeaderForBlockHash(hash)
            })?;

            // 3. Compare block daa score to our target
            match compact_header.daa_score.cmp(&target_daa_score) {
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
                return Err(ReturnAddressError::NoTxAtScore);
            }
        };

        let acceptance_data = self
            .acceptance_data_store
            .get(matching_chain_block_hash)
            .map_err(|_| ReturnAddressError::MissingAcceptanceDataForChainBlock(matching_chain_block_hash))?;

        Ok((matching_chain_block_hash, acceptance_data))
    }

    fn find_containing_block_and_index_from_acceptance_data(
        &self,
        txid: Hash,
        block_acceptance_data: Arc<Vec<MergesetBlockAcceptanceData>>,
    ) -> Option<(Hash, usize)> {
        block_acceptance_data.iter().find_map(|mbad| {
            let tx_arr_index =
                mbad.accepted_transactions.iter().find_map(|tx| (tx.transaction_id == txid).then_some(tx.index_within_block as usize));
            tx_arr_index.map(|index| (mbad.block_hash, index))
        })
    }
}
