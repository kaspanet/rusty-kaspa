use crate::{
    hashing::HasherExtensions,
    tx::{TransactionOutpoint, UtxoEntry, VerifiableTransaction},
};
use kaspa_hashes::HasherBase;
use kaspa_muhash::MuHash;

pub trait MuHashExtensions {
    fn add_transaction(&mut self, tx: &impl VerifiableTransaction, block_daa_score: u64);
    fn add_utxo(&mut self, outpoint: &TransactionOutpoint, entry: &UtxoEntry);
    fn from_transaction(tx: &impl VerifiableTransaction, block_daa_score: u64) -> Self;
    fn from_utxo(outpoint: &TransactionOutpoint, entry: &UtxoEntry) -> Self;
}

impl MuHashExtensions for MuHash {
    fn add_transaction(&mut self, tx: &impl VerifiableTransaction, block_daa_score: u64) {
        let tx_id = tx.id();
        for (input, entry) in tx.populated_inputs() {
            let mut writer = self.remove_element_builder();
            write_utxo(&mut writer, entry, &input.previous_outpoint);
            writer.finalize();
        }
        for (i, output) in tx.outputs().iter().enumerate() {
            let outpoint = TransactionOutpoint::new(tx_id, i as u32);
            let entry = UtxoEntry::new(output.value, output.script_public_key.clone(), block_daa_score, tx.is_coinbase());
            self.add_utxo(&outpoint, &entry);
        }
    }

    fn add_utxo(&mut self, outpoint: &TransactionOutpoint, entry: &UtxoEntry) {
        let mut writer = self.add_element_builder();
        write_utxo(&mut writer, entry, outpoint);
        writer.finalize();
    }

    fn from_transaction(tx: &impl VerifiableTransaction, block_daa_score: u64) -> Self {
        let mut mh = Self::new();
        mh.add_transaction(tx, block_daa_score);
        mh
    }

    fn from_utxo(outpoint: &TransactionOutpoint, entry: &UtxoEntry) -> Self {
        let mut mh = Self::new();
        mh.add_utxo(outpoint, entry);
        mh
    }
}

fn write_utxo(writer: &mut impl HasherBase, entry: &UtxoEntry, outpoint: &TransactionOutpoint) {
    writer
        // Outpoint
        .update(outpoint.transaction_id)
        .update(outpoint.index.to_le_bytes())
        // Utxo entry
        .update(entry.block_daa_score.to_le_bytes())
        .update(entry.amount.to_le_bytes())
        .write_bool(entry.is_coinbase)
        .update(entry.script_public_key.version().to_le_bytes())
        .write_var_bytes(entry.script_public_key.script());
}
