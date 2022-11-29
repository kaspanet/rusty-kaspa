use super::VirtualStateProcessor;
use crate::{
    errors::{
        BlockProcessResult,
        RuleError::{BadAcceptedIDMerkleRoot, BadCoinbaseTransaction, BadUTXOCommitment, InvalidTransactionsInUtxoContext},
    },
    model::stores::{block_transactions::BlockTransactionsStoreReader, daa::DaaStoreReader, ghostdag::GhostdagData},
};
use consensus_core::{
    coinbase::*,
    hashing,
    header::Header,
    muhash::MuHashExtensions,
    tx::{PopulatedTransaction, Transaction, TransactionId, ValidatedTransaction},
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
    },
    BlockHashMap, BlockHashSet, HashMapCustomHasher,
};
use hashes::Hash;
use kaspa_core::trace;
use kaspa_utils::refs::Refs;
use muhash::MuHash;

use rayon::prelude::*;
use std::{iter::once, ops::Deref, sync::Arc};

/// A context for processing the UTXO state of a block with respect to its selected parent.
/// Note this can also be the virtual block.
pub(super) struct UtxoProcessingContext<'a> {
    pub ghostdag_data: Refs<'a, GhostdagData>,
    pub multiset_hash: MuHash,
    pub mergeset_diff: UtxoDiff,
    pub accepted_tx_ids: Vec<TransactionId>,
    pub mergeset_rewards: BlockHashMap<BlockRewardData>,
}

impl<'a> UtxoProcessingContext<'a> {
    pub fn new(ghostdag_data: Refs<'a, GhostdagData>, selected_parent_multiset_hash: MuHash) -> Self {
        let mergeset_size = ghostdag_data.mergeset_size();
        Self {
            ghostdag_data,
            multiset_hash: selected_parent_multiset_hash,
            mergeset_diff: UtxoDiff::default(),
            accepted_tx_ids: Vec::with_capacity(1), // We expect at least the selected parent coinbase tx
            mergeset_rewards: BlockHashMap::with_capacity(mergeset_size),
        }
    }

    pub fn selected_parent(&self) -> Hash {
        self.ghostdag_data.selected_parent
    }
}

impl VirtualStateProcessor {
    /// Calculates UTXO state and transaction acceptance data relative to the selected parent state
    pub(super) fn calculate_utxo_state<V: UtxoView + Sync>(
        self: &Arc<Self>,
        ctx: &mut UtxoProcessingContext,
        selected_parent_utxo_view: &V,
        pov_daa_score: u64,
    ) {
        let selected_parent_transactions = self.block_transactions_store.get(ctx.selected_parent()).unwrap();
        let validated_coinbase = ValidatedTransaction::new_coinbase(&selected_parent_transactions[0]);

        ctx.mergeset_diff.add_transaction(&validated_coinbase, pov_daa_score).unwrap();
        ctx.multiset_hash.add_transaction(&validated_coinbase, pov_daa_score);
        ctx.accepted_tx_ids.push(validated_coinbase.id());

        // TODO: no need to validate selected parent transactions, but only to populate and add,
        // since selected parent txs were already validated as part of selected parent utxo state verification.

        for (merged_block, txs) in once((ctx.selected_parent(), selected_parent_transactions)).chain(
            ctx.ghostdag_data
                .consensus_ordered_mergeset_without_selected_parent(self.ghostdag_store.deref())
                .map(|b| (b, self.block_transactions_store.get(b).unwrap())),
        ) {
            // Create a composed UTXO view from the selected parent UTXO view + the mergeset UTXO diff
            let composed_view = selected_parent_utxo_view.compose(&ctx.mergeset_diff);

            // Validate transactions in current UTXO context
            let validated_transactions = self.validate_transactions_in_parallel(&txs, &composed_view, pov_daa_score);

            let mut block_fee = 0u64;
            for validated_tx in validated_transactions {
                ctx.mergeset_diff.add_transaction(&validated_tx, pov_daa_score).unwrap();
                ctx.multiset_hash.add_transaction(&validated_tx, pov_daa_score);
                ctx.accepted_tx_ids.push(validated_tx.id());
                block_fee += validated_tx.calculated_fee;
            }

            let coinbase_data = self.coinbase_manager.deserialize_coinbase_payload(&txs[0].payload).unwrap();
            ctx.mergeset_rewards.insert(
                merged_block,
                BlockRewardData::new(coinbase_data.subsidy, block_fee, coinbase_data.miner_data.script_public_key),
            );
        }

        // Make sure accepted tx ids are sorted before building the merkle root
        // NOTE: when subnetworks will be enabled, the sort should consider them in order to allow grouping under a merkle subtree
        ctx.accepted_tx_ids.sort();
    }

    /// Verify that the current block fully respects its own UTXO view. We define a block as
    /// UTXO valid if all the following conditions hold:
    ///     1. The block header includes the expected `utxo_commitment`.
    ///     2. The block header includes the expected `accepted_id_merkle_root`.
    ///     3. The block coinbase transaction rewards the mergeset blocks correctly.
    ///     4. All non-coinbase block transactions are valid against its own UTXO view.
    pub(super) fn verify_expected_utxo_state<V: UtxoView + Sync>(
        self: &Arc<Self>,
        ctx: &mut UtxoProcessingContext,
        selected_parent_utxo_view: &V,
        header: &Header,
    ) -> BlockProcessResult<()> {
        // Verify header UTXO commitment
        let expected_commitment = ctx.multiset_hash.finalize();
        if expected_commitment != header.utxo_commitment {
            return Err(BadUTXOCommitment(header.hash, header.utxo_commitment, expected_commitment));
        }
        // trace!("correct commitment: {}, {}", header.hash, expected_commitment);

        // Verify header accepted_id_merkle_root
        let expected_accepted_id_merkle_root = merkle::calc_merkle_root(ctx.accepted_tx_ids.iter().copied());
        if expected_accepted_id_merkle_root != header.accepted_id_merkle_root {
            return Err(BadAcceptedIDMerkleRoot(header.hash, header.accepted_id_merkle_root, expected_accepted_id_merkle_root));
        }

        let txs = self.block_transactions_store.get(header.hash).unwrap();

        // Verify coinbase transaction
        self.verify_coinbase_transaction(
            &txs[0],
            header.daa_score,
            &ctx.ghostdag_data,
            &ctx.mergeset_rewards,
            &self.daa_store.get_mergeset_non_daa(header.hash).unwrap(),
        )?;

        // Verify all transactions are valid in context (TODO: skip validation when becoming selected parent)
        let current_utxo_view = selected_parent_utxo_view.compose(&ctx.mergeset_diff);
        let validated_transactions = self.validate_transactions_in_parallel(&txs, &current_utxo_view, header.daa_score);
        if validated_transactions.len() < txs.len() - 1 {
            // Some non-coinbase transactions are invalid
            return Err(InvalidTransactionsInUtxoContext(txs.len() - 1 - validated_transactions.len(), txs.len() - 1));
        }

        Ok(())
    }

    fn verify_coinbase_transaction(
        self: &Arc<Self>,
        coinbase: &Transaction,
        daa_score: u64,
        ghostdag_data: &GhostdagData,
        mergeset_rewards: &BlockHashMap<BlockRewardData>,
        mergeset_non_daa: &BlockHashSet,
    ) -> BlockProcessResult<()> {
        // Extract only miner data from the provided coinbase
        let miner_data = self.coinbase_manager.deserialize_coinbase_payload(&coinbase.payload).unwrap().miner_data;
        let expected_coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(daa_score, miner_data, ghostdag_data, mergeset_rewards, mergeset_non_daa)
            .unwrap()
            .tx;
        // trace!(
        //     "mergeset: {} blues, {} reds, {} non-DAA",
        //     ghostdag_data.mergeset_blues.len(),
        //     ghostdag_data.mergeset_reds.len(),
        //     mergeset_non_daa.len()
        // );
        if hashing::tx::hash(coinbase) != hashing::tx::hash(&expected_coinbase) {
            Err(BadCoinbaseTransaction)
        } else {
            Ok(())
        }
    }

    /// Validates transactions against the provided `utxo_view` and returns a vector with all transactions
    /// which passed the validation
    pub fn validate_transactions_in_parallel<'a, V: UtxoView + Sync>(
        self: &Arc<Self>,
        txs: &'a Vec<Transaction>,
        utxo_view: &V,
        pov_daa_score: u64,
    ) -> Vec<ValidatedTransaction<'a>> {
        self.thread_pool.install(|| {
            txs
                .par_iter() // We can do this in parallel without complications since block body validation already ensured
                            // that all txs within each block are independent
                .skip(1) // Skip the coinbase tx.
                .filter_map(|tx| self.validate_transaction_in_utxo_context(tx, &utxo_view, pov_daa_score))
                .collect()
        })
    }

    /// Attempts to populate the transaction with UTXO entries and performs all tx validations
    fn validate_transaction_in_utxo_context<'a>(
        self: &Arc<Self>,
        transaction: &'a Transaction,
        utxo_view: &impl UtxoView,
        pov_daa_score: u64,
    ) -> Option<ValidatedTransaction<'a>> {
        let mut entries = Vec::with_capacity(transaction.inputs.len());
        for input in transaction.inputs.iter() {
            if let Some(entry) = utxo_view.get(&input.previous_outpoint) {
                entries.push(entry);
            } else {
                return None; // Missing inputs
            }
        }
        let populated_tx = PopulatedTransaction::new(transaction, entries);
        let res = self.transaction_validator.validate_populated_transaction_and_get_fee(&populated_tx, pov_daa_score);
        match res {
            Ok(calculated_fee) => Some(populated_tx.to_validated(calculated_fee)),
            Err(tx_rule_error) => {
                trace!("tx rule error {} for tx {}", tx_rule_error, transaction.id());
                None
            }
        }
    }
}
