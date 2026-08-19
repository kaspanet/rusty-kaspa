use crate::{consensus::test_consensus::TestConsensus, model::stores::block_transactions::BlockTransactionsStoreReader};
use kaspa_consensus_core::{
    BlockHashSet,
    api::ConsensusApi,
    block::{Block, BlockTemplate, MutableBlock, TemplateBuildMode, TemplateTransactionSelector},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    constants::TX_VERSION_TOCCATA,
    subnets::SubnetworkId,
    tx::{
        ComputeCommit, ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, scriptvec,
    },
};
use kaspa_hashes::Hash;
use std::{collections::VecDeque, thread::JoinHandle};

pub(super) struct OnetimeTxSelector {
    txs: Option<Vec<Transaction>>,
}

impl OnetimeTxSelector {
    pub(super) fn new(txs: Vec<Transaction>) -> Self {
        Self { txs: Some(txs) }
    }
}

impl TemplateTransactionSelector for OnetimeTxSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        self.txs.take().unwrap()
    }

    fn reject_selection(&mut self, _tx_id: kaspa_consensus_core::tx::TransactionId) {
        unimplemented!()
    }

    fn is_successful(&self) -> bool {
        true
    }
}

pub(super) struct TestContext {
    pub consensus: TestConsensus,
    join_handles: Vec<JoinHandle<()>>,
    miner_data: MinerData,
    simulated_time: u64,
    current_templates: VecDeque<BlockTemplate>,
    current_tips: BlockHashSet,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        self.consensus.shutdown(std::mem::take(&mut self.join_handles));
    }
}

impl TestContext {
    pub(super) fn new(consensus: TestConsensus) -> Self {
        let join_handles = consensus.init();
        let genesis_hash = consensus.params().genesis.hash;
        let simulated_time = consensus.params().genesis.timestamp;
        Self {
            consensus,
            join_handles,
            miner_data: new_miner_data(),
            simulated_time,
            current_templates: Default::default(),
            current_tips: BlockHashSet::from_iter([genesis_hash]),
        }
    }

    pub(super) fn assert_row_parents(&mut self) -> &mut Self {
        for t in self.current_templates.iter() {
            assert_eq!(self.current_tips, BlockHashSet::from_iter(t.block.header.direct_parents().iter().copied()));
        }
        self
    }

    pub(super) fn assert_tips(&mut self) -> &mut Self {
        assert_eq!(BlockHashSet::from_iter(self.consensus.get_tips().into_iter()), self.current_tips);
        self
    }

    pub(super) fn assert_virtual_parents_subset(&mut self) -> &mut Self {
        assert!(self.consensus.get_virtual_parents().is_subset(&self.current_tips));
        self
    }

    pub(super) async fn build_and_insert_disqualified_chain(&mut self, mut parents: Vec<Hash>, len: usize) -> Hash {
        // The chain will be disqualified since build_block_with_parents builds utxo-invalid blocks
        for _ in 0..len {
            self.simulated_time += self.consensus.params().target_time_per_block();
            let b = self.build_block_with_parents(parents, 0, self.simulated_time);
            parents = vec![b.header.hash];
            self.validate_and_insert_block(b.to_immutable()).await;
        }
        parents[0]
    }

    pub(super) fn build_block_template_row(&mut self, nonces: impl Iterator<Item = usize>) -> &mut Self {
        for nonce in nonces {
            self.simulated_time += self.consensus.params().target_time_per_block();
            self.current_templates.push_back(self.build_block_template(nonce as u64, self.simulated_time));
        }
        self
    }

    pub(super) async fn validate_and_insert_row(&mut self) -> &mut Self {
        self.current_tips.clear();
        while let Some(t) = self.current_templates.pop_front() {
            self.current_tips.insert(t.block.header.hash);
            self.validate_and_insert_block(t.block.to_immutable()).await;
        }
        self
    }

    pub(super) fn build_block_template(&self, nonce: u64, timestamp: u64) -> BlockTemplate {
        let mut t = self
            .consensus
            .build_block_template(
                self.miner_data.clone(),
                Box::new(OnetimeTxSelector::new(Default::default())),
                TemplateBuildMode::Standard,
            )
            .unwrap();
        t.block.header.timestamp = timestamp;
        t.block.header.nonce = nonce;
        t.block.header.finalize();
        t
    }

    pub(super) fn build_block_with_parents(&self, parents: Vec<Hash>, nonce: u64, timestamp: u64) -> MutableBlock {
        let mut b = self.consensus.build_block_with_parents_and_transactions(
            kaspa_consensus_core::blockhash::NONE,
            parents,
            Default::default(),
        );
        b.header.timestamp = timestamp;
        b.header.nonce = nonce;
        b.header.finalize();
        b
    }

    pub(super) async fn validate_and_insert_block(&mut self, block: Block) -> &mut Self {
        let status = self.consensus.validate_and_insert_block(block).virtual_state_task.await.unwrap();
        assert!(status.has_block_body());
        self
    }

    pub(super) fn assert_tips_num(&mut self, expected_num: usize) -> &mut Self {
        assert_eq!(BlockHashSet::from_iter(self.consensus.get_tips().into_iter()).len(), expected_num);
        self
    }

    pub(super) fn assert_valid_utxo_tip(&mut self) -> &mut Self {
        assert!(self.consensus.body_tips().iter().copied().any(|h| self.consensus.block_status(h) == BlockStatus::StatusUTXOValid));
        self
    }

    pub(super) async fn add_utxo_valid_block_with_parents(&self, hash: Hash, parents: Vec<Hash>, txs: Vec<Transaction>) {
        self.consensus.add_utxo_valid_block_with_parents(hash, parents, txs).await.unwrap();
    }

    /// Builds and inserts a UTXO-valid block whose miner payout script is OP_TRUE.
    ///
    /// This is useful for fixtures that need to spend coinbase outputs without introducing
    /// signature construction into the test setup.
    pub(super) async fn add_op_true_block(&mut self, hash: Hash, parent: Hash, txs: Vec<Transaction>) {
        let block: Block =
            self.consensus.build_utxo_valid_block_with_parents(hash, vec![parent], op_true_miner_data(), txs).to_immutable();
        self.validate_and_insert_block(block).await.assert_valid_utxo_tip();
    }

    /// Creates a Toccata user-lane transaction that spends the first positive coinbase
    /// output in `source_block` and pays it back to OP_TRUE.
    ///
    /// The value is preserved so the transaction has zero fee and, for a one-input /
    /// one-output spend, zero storage-mass commitment.
    pub(super) fn spend_coinbase_output(&self, source_block: Hash, lane: SubnetworkId, payload: Vec<u8>) -> Transaction {
        let txs = self.consensus.block_transactions_store.get(source_block).unwrap();
        let coinbase = &txs[0];
        let (output_index, output) = coinbase
            .outputs
            .iter()
            .enumerate()
            .find(|(_, output)| output.value > 0)
            .expect("test block should contain a spendable coinbase output");

        Transaction::new(
            TX_VERSION_TOCCATA,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint::new(coinbase.id(), output_index as u32),
                signature_script: vec![],
                sequence: u64::MAX,
                compute_commit: ComputeCommit::ComputeBudget(0.into()),
            }],
            vec![TransactionOutput::new(output.value, op_true_script_public_key())],
            0,
            lane,
            0,
            payload,
        )
    }
}

pub(super) fn new_miner_data() -> MinerData {
    let secp = secp256k1::Secp256k1::new();
    let mut rng = rand::thread_rng();
    let (_sk, pk) = secp.generate_keypair(&mut rng);
    let script = ScriptVec::from_slice(&pk.serialize());
    MinerData::new(ScriptPublicKey::new(0, script), vec![])
}

/// Creates miner data with an OP_TRUE payout script, making the resulting
/// coinbase outputs deliberately easy to spend in consensus fixtures.
fn op_true_miner_data() -> MinerData {
    MinerData::new(op_true_script_public_key(), vec![])
}

fn op_true_script_public_key() -> ScriptPublicKey {
    ScriptPublicKey::new(0, scriptvec!(0x51u8))
}
