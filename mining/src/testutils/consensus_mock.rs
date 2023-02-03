use consensus_core::{
    api::ConsensusApi,
    block::{Block, BlockTemplate, MutableBlock},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    constants::BLOCK_VERSION,
    errors::{
        block::{BlockProcessResult, RuleError},
        coinbase::CoinbaseResult,
        tx::{TxResult, TxRuleError},
    },
    header::Header,
    mass::transaction_estimated_serialized_size,
    merkle::calc_hash_merkle_root,
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use futures_util::future::BoxFuture;
use hashes::ZERO_HASH;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc, time::SystemTime};

use super::coinbase_mock::CoinbaseManagerMock;

pub(crate) struct ConsensusMock {
    transactions: RwLock<HashMap<TransactionId, Arc<Transaction>>>,
    statuses: RwLock<HashMap<TransactionId, TxResult<()>>>,
    utxos: RwLock<UtxoCollection>,
}

impl ConsensusMock {
    pub(crate) fn new() -> Self {
        Self {
            transactions: RwLock::new(HashMap::default()),
            statuses: RwLock::new(HashMap::default()),
            utxos: RwLock::new(HashMap::default()),
        }
    }

    pub(crate) fn set_status(&self, transaction_id: TransactionId, status: TxResult<()>) {
        self.statuses.write().insert(transaction_id, status);
    }

    pub(crate) fn add_transaction(&self, transaction: Transaction, block_daa_score: u64) {
        let transaction = MutableTransaction::from_tx(transaction);
        let mut transactions = self.transactions.write();
        let mut utxos = self.utxos.write();

        // Remove the spent UTXOs
        transaction.tx.inputs.iter().for_each(|x| {
            utxos.remove(&x.previous_outpoint);
        });
        // Create the new UTXOs
        transaction.tx.outputs.iter().enumerate().for_each(|(i, x)| {
            utxos.insert(
                TransactionOutpoint::new(transaction.id(), i as u32),
                UtxoEntry::new(x.value, x.script_public_key.clone(), block_daa_score, transaction.tx.is_coinbase()),
            );
        });
        // Register the transaction
        transactions.insert(transaction.id(), transaction.tx);
    }

    pub(crate) fn can_finance_transaction(&self, transaction: &MutableTransaction) -> bool {
        let utxos = self.utxos.read();
        for outpoint in transaction.missing_outpoints() {
            if !utxos.contains_key(&outpoint) {
                return false;
            }
        }
        true
    }

    // fn t() {
    //             // TODO: Replace this hack by a call to build the script (some txscript.PayToAddrScript(payAddress) equivalent).
    //     //       See app\rpc\rpchandlers\get_block_template.go HandleGetBlockTemplate
    //     const ADDRESS_PUBLIC_KEY_SCRIPT_PUBLIC_KEY_VERSION: u16 = 0;
    //     const OP_CHECK_SIG: u8 = 172;
    //     let mut script_addr = request.pay_address.payload.clone();
    //     let mut pay_to_pub_key_script = Vec::with_capacity(34);
    //     pay_to_pub_key_script.push(u8::try_from(script_addr.len()).unwrap());
    //     pay_to_pub_key_script.append(&mut script_addr);
    //     pay_to_pub_key_script.push(OP_CHECK_SIG);

    //     let script = ScriptVec::from_vec(pay_to_pub_key_script);

    //     let script_public_key = ScriptPublicKey::new(ADDRESS_PUBLIC_KEY_SCRIPT_PUBLIC_KEY_VERSION, script);

    // }
}

impl ConsensusApi for ConsensusMock {
    fn build_block_template(self: Arc<Self>, miner_data: MinerData, mut txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError> {
        let coinbase_manager = CoinbaseManagerMock::new();
        let coinbase = coinbase_manager.expected_coinbase_transaction(miner_data.clone());
        txs.insert(0, coinbase.tx);
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        let hash_merkle_root = calc_hash_merkle_root(txs.iter());
        let header = Header::new(
            BLOCK_VERSION,
            vec![],
            hash_merkle_root,
            ZERO_HASH,
            ZERO_HASH,
            now,
            123456789u32,
            0,
            0,
            0.into(),
            0,
            ZERO_HASH,
        );
        let mutable_block = MutableBlock::new(header, txs);

        Ok(BlockTemplate::new(mutable_block, miner_data, coinbase.has_red_reward, now))
    }

    fn validate_and_insert_block(
        self: Arc<Self>,
        _block: Block,
        _update_virtual: bool,
    ) -> BoxFuture<'static, BlockProcessResult<BlockStatus>> {
        unimplemented!()
    }

    fn validate_mempool_transaction_and_populate(self: Arc<Self>, mutable_tx: &mut MutableTransaction) -> TxResult<()> {
        // If a predefined status was registered to simulate an error, return it right away
        if let Some(status) = self.statuses.read().get(&mutable_tx.id()) {
            if status.is_err() {
                return status.clone();
            }
        }
        let utxos = self.utxos.read();
        let mut has_missing_outpoints = false;
        for i in 0..mutable_tx.tx.inputs.len() {
            // Keep existing entries
            if mutable_tx.entries[i].is_some() {
                continue;
            }
            // Try add missing entries
            if let Some(entry) = utxos.get(&mutable_tx.tx.inputs[i].previous_outpoint) {
                mutable_tx.entries[i] = Some(entry.clone());
            } else {
                has_missing_outpoints = true;
            }
        }
        if has_missing_outpoints {
            return Err(TxRuleError::MissingTxOutpoints);
        }
        // At this point we know all UTXO entries are populated, so we can safely calculate the fee
        let total_in: u64 = mutable_tx.entries.iter().map(|x| x.as_ref().unwrap().amount).sum();
        let total_out: u64 = mutable_tx.tx.outputs.iter().map(|x| x.value).sum();
        let calculated_fee = total_in - total_out;
        mutable_tx.calculated_fee = Some(calculated_fee);
        Ok(())
    }

    fn calculate_transaction_mass(self: Arc<Self>, transaction: &Transaction) -> u64 {
        if transaction.is_coinbase() {
            0
        } else {
            transaction_estimated_serialized_size(transaction)
        }
    }

    fn get_virtual_daa_score(self: Arc<Self>) -> u64 {
        0
    }

    fn modify_coinbase_payload(self: Arc<Self>, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        let coinbase_manager = CoinbaseManagerMock::new();
        Ok(coinbase_manager.modify_coinbase_payload(payload, miner_data))
    }
}
