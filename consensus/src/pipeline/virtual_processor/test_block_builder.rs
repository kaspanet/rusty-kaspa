use crate::model::stores::{
    pruning::PruningStoreReader, utxo_multisets::UtxoMultisetsStoreReader, virtual_state::VirtualStateStoreReader,
};
use kaspa_consensus_core::{
    block::BlockTemplate, coinbase::MinerData, errors::block::RuleError, tx::Transaction, utxo::utxo_view::UtxoViewComposition,
};
use kaspa_hashes::Hash;

use super::VirtualStateProcessor;

impl VirtualStateProcessor {
    /// Test-only helper method for building a block template with specific parents
    pub(crate) fn build_block_template_with_parents(
        &self,
        parents: Vec<Hash>,
        miner_data: MinerData,
        txs: Vec<Transaction>,
    ) -> Result<BlockTemplate, RuleError> {
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let virtual_read = self.virtual_stores.read();
        let state = virtual_read.state.get().unwrap();
        let finality_point = self.virtual_finality_point(&state.ghostdag_data, pruning_point);
        let sink = state.ghostdag_data.selected_parent;
        let mut accumulated_diff = state.utxo_diff.clone().to_reversed();
        let (new_sink, virtual_parent_candidates) =
            self.sink_search_algorithm(&virtual_read, &mut accumulated_diff, sink, parents, finality_point, pruning_point);
        let (virtual_parents, virtual_ghostdag_data) = self.pick_virtual_parents(new_sink, virtual_parent_candidates, pruning_point);
        let sink_multiset = self.utxo_multisets_store.get(new_sink).unwrap();
        let new_virtual_state =
            self.calculate_virtual_state(&virtual_read, virtual_parents, virtual_ghostdag_data, sink_multiset, &mut accumulated_diff)?;
        let virtual_utxo_view = (&virtual_read.utxo_set).compose(accumulated_diff);
        self.validate_block_template_transactions(&txs, &new_virtual_state, &virtual_utxo_view)?;
        drop(virtual_read);
        self.build_block_template_from_virtual_state(new_virtual_state, miner_data, txs)
    }
}
