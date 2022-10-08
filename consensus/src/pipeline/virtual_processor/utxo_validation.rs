use super::VirtualStateProcessor;
use crate::{
    errors::{
        BlockProcessResult,
        RuleError::{BadAcceptedIDMerkleRoot, BadUTXOCommitment, InvalidTransactionsInUtxoContext},
    },
    model::stores::{block_transactions::BlockTransactionsStoreReader, ghostdag::GhostdagData},
};
use consensus_core::{
    header::Header,
    tx::{PopulatedTransaction, Transaction, ValidatedTransaction},
    utxo::utxo_view::UtxoView,
    BlockHashMap,
};
use hashes::Hash;
use kaspa_core::trace;
use muhash::MuHash;

use rayon::prelude::*;
use std::sync::Arc;

impl VirtualStateProcessor {
    /// Verify that the current block fully respects its own UTXO view. We define a block as
    /// UTXO valid if all the following conditions hold:
    ///     1. The block header includes the expected `utxo_commitment`.
    ///     2. The block header includes the expected `accepted_id_merkle_root`.
    ///     3. The block coinbase transaction rewards the mergeset blocks correctly.
    ///     4. All non-coinbase block transactions are valid against its own UTXO view.
    pub fn verify_utxo_validness_requirements<V: UtxoView + Sync>(
        self: &Arc<VirtualStateProcessor>,
        utxo_view: &V,
        header: &Header,
        mergeset_data: &GhostdagData,
        multiset_hash: &mut MuHash,
        mut accepted_tx_ids: Vec<Hash>,
        mergeset_fees: BlockHashMap<u64>,
    ) -> BlockProcessResult<()> {
        // Verify header UTXO commitment
        let expected_commitment = multiset_hash.finalize();
        if expected_commitment != header.utxo_commitment {
            return Err(BadUTXOCommitment(header.hash, header.utxo_commitment, expected_commitment));
        } else {
            trace!("correct commitment: {}, {}", header.hash, expected_commitment);
        }

        // Verify header accepted_id_merkle_root
        accepted_tx_ids.sort();
        let expected_accepted_id_merkle_root = merkle::calc_merkle_root(accepted_tx_ids.iter().copied());
        if expected_accepted_id_merkle_root != header.accepted_id_merkle_root {
            return Err(BadAcceptedIDMerkleRoot(header.hash, header.accepted_id_merkle_root, expected_accepted_id_merkle_root));
        }

        let txs = self.block_transactions_store.get(header.hash).unwrap();

        // Verify coinbase transaction
        self.verify_coinbase_transaction(&txs[0], mergeset_data, mergeset_fees)?;

        // Verify all transactions are valid in context (TODO: skip validation when becoming selected parent)
        let validated_transactions = self.validate_transactions_in_parallel(&txs, &utxo_view, header.daa_score);
        if validated_transactions.len() < txs.len() - 1 {
            // Some transactions were invalid
            return Err(InvalidTransactionsInUtxoContext(txs.len() - 1 - validated_transactions.len(), txs.len() - 1));
        }

        Ok(())
    }

    fn verify_coinbase_transaction(
        self: &Arc<VirtualStateProcessor>,
        coinbase_tx: &Transaction,
        mergeset_data: &GhostdagData,
        mergeset_fees: BlockHashMap<u64>,
    ) -> BlockProcessResult<()> {
        // TODO: build expected coinbase using `mergeset_fees` and compare with the given tx
        // Return `Err(BadCoinbaseTransaction)` if the expected and actual defer
        Ok(())
    }

    /// Validates transactions against the provided `utxo_view` and returns a vector with all transactions
    /// which passed the validation
    pub fn validate_transactions_in_parallel<'a, V: UtxoView + Sync>(
        self: &Arc<VirtualStateProcessor>,
        txs: &'a Vec<Transaction>,
        utxo_view: &V,
        pov_daa_score: u64,
    ) -> Vec<ValidatedTransaction<'a>> {
        txs
            .par_iter() // We can do this in parallel without complications since block body validation already ensured 
                        // that all txs in the block are independent 
            .skip(1) // Skip the coinbase tx. 
            .filter_map(|tx| self.validate_transaction_in_utxo_context(tx, &utxo_view, pov_daa_score))
            .collect()
    }

    /// Attempts to populate the transaction with UTXO entries and performs all tx validations
    fn validate_transaction_in_utxo_context<'a>(
        self: &Arc<VirtualStateProcessor>,
        transaction: &'a Transaction,
        utxo_view: &impl UtxoView,
        pov_daa_score: u64,
    ) -> Option<ValidatedTransaction<'a>> {
        let mut entries = Vec::with_capacity(transaction.inputs.len());
        for input in transaction.inputs.iter() {
            if let Some(entry) = utxo_view.get(&input.previous_outpoint) {
                entries.push(entry.clone());
            } else {
                trace!("missing UTXO entry for outpoint {}", input.previous_outpoint);
                break;
            }
        }
        if entries.len() < transaction.inputs.len() {
            // Missing inputs
            return None;
        }
        let populated_tx = PopulatedTransaction::new(transaction, entries);
        let res = self.transaction_validator.validate_populated_transaction_and_get_fee(&populated_tx, pov_daa_score);
        match res {
            Ok(calculated_fee) => Some(populated_tx.to_validated(calculated_fee)),
            Err(tx_rule_error) => {
                trace!("tx rule error {} for tx {}", tx_rule_error, transaction.id());
                None // TODO: add to acceptance data as unaccepted tx
            }
        }
    }
}
