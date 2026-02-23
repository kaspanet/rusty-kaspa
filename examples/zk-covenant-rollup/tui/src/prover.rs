use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use kaspa_hashes::Hash;
use kaspa_rpc_core::GetVirtualChainFromBlockV2Response;
use zk_covenant_rollup_core::permission_tree::required_depth;
use zk_covenant_rollup_core::PublicInput;
use zk_covenant_rollup_core::{
    action::{ActionHeader, EntryAction, ExitAction, TransferAction, OP_ENTRY, OP_EXIT, OP_TRANSFER},
    bytes_to_words_ref, is_action_tx_id, pay_to_pubkey_spk, perm_leaf_hash,
    permission_tree::StreamingPermTreeBuilder,
    smt::Smt,
    state::{AccountWitness, StateRoot},
};
use zk_covenant_rollup_host::mock_chain::from_bytes;
use zk_covenant_rollup_host::mock_tx::{ActionWitness, EntryWitnessData, ExitWitnessData, TransferWitnessData, ZkTransaction};
use zk_covenant_rollup_host::prove::ProveInput;

/// Tracks L2 state and processes chain data for proving.
pub struct RollupProver {
    /// Sparse Merkle Tree holding account balances.
    pub smt: Smt,
    /// Current state root ([u32; 8]).
    pub state_root: StateRoot,
    /// Current sequence commitment (Hash).
    pub seq_commitment: Hash,
    /// Covenant ID as [u32; 8] (for host crate compatibility).
    pub covenant_id: [u32; 8],
    /// Covenant ID as bytes.
    pub covenant_id_bytes: [u8; 32],
    /// Hash of the last block we have processed.
    pub last_processed_block: Hash,
    /// Permission tree builder for exits.
    pub perm_builder: StreamingPermTreeBuilder,
    /// All ZkTransactions from the last processing run (grouped by block).
    pub last_block_txs: Vec<Vec<ZkTransaction>>,

    // ── Proving accumulator ──
    // These track ALL data since the last proof, so we can snapshot and prove.
    /// State root at the start of the current proving window.
    pub prev_proved_state_root: StateRoot,
    /// Sequence commitment at the start of the current proving window.
    pub prev_proved_seq_commitment: Hash,
    /// All block txs accumulated since the last proof.
    pub accumulated_block_txs: Vec<Vec<ZkTransaction>>,
    /// Permission tree builder for the current proving window (for exits).
    pub accumulated_perm_builder: StreamingPermTreeBuilder,
    /// (spk_bytes, amount) for each exit in the current proving window.
    pub accumulated_exit_data: Vec<(Vec<u8>, u64)>,
}

/// Data returned by `take_prove_snapshot`, combining the prove input
/// with the permission redeem script (if exits occurred in this batch).
pub struct ProveSnapshot {
    pub input: ProveInput,
    /// Full permission redeem script bytes (only when exits occurred).
    pub perm_redeem_script: Option<Vec<u8>>,
    /// (spk_bytes, amount) for each exit in the proving window (empty if none).
    pub perm_exit_data: Vec<(Vec<u8>, u64)>,
}

/// Result of processing a VCC v2 response.
pub struct ProcessResult {
    pub blocks_processed: usize,
    pub txs_processed: usize,
    pub actions_found: usize,
    pub new_state_root: StateRoot,
    pub new_seq_commitment: Hash,
}

impl RollupProver {
    pub fn new(covenant_id: Hash, initial_state_root: StateRoot, initial_seq_commitment: Hash, starting_block: Hash) -> Self {
        let covenant_id_words = from_bytes(covenant_id.as_bytes());
        Self {
            smt: Smt::new(),
            state_root: initial_state_root,
            seq_commitment: initial_seq_commitment,
            covenant_id: covenant_id_words,
            covenant_id_bytes: covenant_id.as_bytes(),
            last_processed_block: starting_block,
            perm_builder: StreamingPermTreeBuilder::new(),
            last_block_txs: Vec::new(),
            prev_proved_state_root: initial_state_root,
            prev_proved_seq_commitment: initial_seq_commitment,
            accumulated_block_txs: Vec::new(),
            accumulated_perm_builder: StreamingPermTreeBuilder::new(),
            accumulated_exit_data: Vec::new(),
        }
    }

    /// Number of blocks accumulated since the last proof.
    pub fn accumulated_blocks(&self) -> usize {
        self.accumulated_block_txs.len()
    }

    /// Take a snapshot of the accumulated data for proving, then reset the
    /// accumulator so new chain data can continue flowing in.
    ///
    /// Returns `None` if there are no blocks to prove.
    pub fn take_prove_snapshot(&mut self) -> Option<ProveSnapshot> {
        if self.accumulated_block_txs.is_empty() {
            return None;
        }

        let public_input = PublicInput {
            prev_state_hash: self.prev_proved_state_root,
            prev_seq_commitment: from_bytes(self.prev_proved_seq_commitment.as_bytes()),
            covenant_id: self.covenant_id,
        };

        let block_txs = std::mem::take(&mut self.accumulated_block_txs);

        // Take the accumulated perm builder (replace with fresh one)
        let old_perm_builder = std::mem::replace(&mut self.accumulated_perm_builder, StreamingPermTreeBuilder::new());
        let perm_count = old_perm_builder.leaf_count();
        let (perm_redeem_script_len, perm_redeem_script) = if perm_count > 0 {
            let perm_root = old_perm_builder.finalize();
            let depth = required_depth(perm_count as usize);
            let padded_root = zk_covenant_rollup_core::permission_tree::pad_to_depth(perm_root, perm_count, depth);
            let redeem = zk_covenant_rollup_core::permission_script::build_permission_redeem_bytes_converged(
                &padded_root,
                perm_count as u64,
                depth,
                zk_covenant_rollup_core::MAX_DELEGATE_INPUTS,
            );
            let len = Some(redeem.len() as i64);
            (len, Some(redeem))
        } else {
            (None, None)
        };

        // Take exit data for this proving window
        let perm_exit_data = std::mem::take(&mut self.accumulated_exit_data);

        // Advance the proving window start to current state
        self.prev_proved_state_root = self.state_root;
        self.prev_proved_seq_commitment = self.seq_commitment;

        Some(ProveSnapshot {
            input: ProveInput { public_input, block_txs, perm_redeem_script_len },
            perm_redeem_script,
            perm_exit_data,
        })
    }

    /// Process a VCC v2 response, converting RPC transactions to ZkTransactions
    /// and updating the L2 state (SMT + seq_commitment).
    pub fn process_chain_response(&mut self, response: &GetVirtualChainFromBlockV2Response) -> ProcessResult {
        let mut blocks_processed = 0;
        let mut txs_processed = 0;
        let mut actions_found = 0;

        self.last_block_txs.clear();

        for (block_idx, block) in response.chain_block_accepted_transactions.iter().enumerate() {
            let mut zk_txs = Vec::new();

            for rpc_tx in &block.accepted_transactions {
                // Try to convert the optional RPC transaction to a consensus Transaction
                let tx = match rpc_optional_to_transaction(rpc_tx) {
                    Some(tx) => tx,
                    None => continue,
                };

                let tx_id_words = bytes_to_words_ref(&tx.id().as_bytes());
                let version = tx.version;

                // Check if this is an action transaction (V1+ with action prefix)
                let witness = if version >= 1 && is_action_tx_id(&tx_id_words) {
                    actions_found += 1;
                    self.build_action_witness(&tx)
                } else {
                    None
                };

                zk_txs.push(ZkTransaction { tx, witness });
                txs_processed += 1;
            }

            // Read seq_commitment from block header (no need to compute — OpSeqCommit reads it on-chain)
            if let Some(air) = block.chain_block_header.accepted_id_merkle_root {
                self.seq_commitment = air;
            }

            // Update last processed block hash
            if block_idx < response.added_chain_block_hashes.len() {
                self.last_processed_block = response.added_chain_block_hashes[block_idx];
            }

            // Also accumulate for proving (clone into the proving window)
            self.accumulated_block_txs.push(zk_txs.clone());
            self.last_block_txs.push(zk_txs);
            blocks_processed += 1;
        }

        self.state_root = self.smt.root();

        ProcessResult {
            blocks_processed,
            txs_processed,
            actions_found,
            new_state_root: self.state_root,
            new_seq_commitment: self.seq_commitment,
        }
    }

    /// Build an ActionWitness for an action transaction and apply the state transition.
    fn build_action_witness(&mut self, tx: &Transaction) -> Option<ActionWitness> {
        let payload = &tx.payload;
        if payload.len() < 4 {
            return None;
        }

        let payload_words: Vec<u32> = payload.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();

        if payload_words.len() < ActionHeader::WORDS {
            return None;
        }

        let header = ActionHeader::from_words_ref(payload_words[..ActionHeader::WORDS].try_into().unwrap());
        if !header.is_valid_version() {
            return None;
        }

        match header.operation {
            OP_TRANSFER => self.process_transfer(&payload_words, tx),
            OP_ENTRY => self.process_entry(&payload_words, tx),
            OP_EXIT => self.process_exit(&payload_words, tx),
            _ => None,
        }
    }

    fn process_transfer(&mut self, payload_words: &[u32], tx: &Transaction) -> Option<ActionWitness> {
        if payload_words.len() < ActionHeader::WORDS + TransferAction::WORDS {
            return None;
        }

        let action = TransferAction::from_words(payload_words[ActionHeader::WORDS..][..TransferAction::WORDS].try_into().unwrap());
        if !action.is_valid() {
            return None;
        }

        let source_pk = action.source;
        let dest_pk = action.destination;
        let amount = action.amount;

        // Check source balance
        let source_balance = self.smt.get(&source_pk)?;
        if source_balance < amount {
            return None; // Insufficient balance
        }

        // Build source witness from current state
        let source_proof = self.smt.prove(&source_pk);
        let source_witness = AccountWitness::new(source_pk, source_balance, source_proof);

        // Update source balance (intermediate state)
        let new_source_balance = source_balance - amount;
        self.smt.upsert(source_pk, new_source_balance);

        // Build dest witness from intermediate state
        let dest_balance = self.smt.get(&dest_pk).unwrap_or(0);
        let dest_proof = self.smt.prove(&dest_pk);
        let dest_exists = self.smt.get(&dest_pk).is_some();
        let dest_witness = if dest_exists {
            AccountWitness::new(dest_pk, dest_balance, dest_proof)
        } else {
            AccountWitness::new([0u32; 8], 0, dest_proof)
        };

        // Update dest balance
        let new_dest_balance = dest_balance + amount;
        self.smt.upsert(dest_pk, new_dest_balance);

        // Build prev_tx from the transaction's first input
        let prev_tx = build_prev_tx_for_action(tx, &source_pk);

        Some(ActionWitness::Transfer(Box::new(TransferWitnessData {
            source: source_witness,
            dest: dest_witness,
            prev_tx,
            prev_output_index: 0,
        })))
    }

    fn process_entry(&mut self, payload_words: &[u32], tx: &Transaction) -> Option<ActionWitness> {
        if payload_words.len() < ActionHeader::WORDS + EntryAction::WORDS {
            return None;
        }

        let action = EntryAction::from_words(payload_words[ActionHeader::WORDS..][..EntryAction::WORDS].try_into().unwrap());

        let dest_pk = action.destination;

        // Get deposit amount from first output value
        let deposit_amount = tx.outputs.first().map(|o| o.value).unwrap_or(0);
        if deposit_amount == 0 {
            return None;
        }

        // Build dest witness
        let dest_exists = self.smt.get(&dest_pk).is_some();
        let dest_balance = self.smt.get(&dest_pk).unwrap_or(0);
        let dest_proof = self.smt.prove(&dest_pk);
        let dest_witness = if dest_exists {
            AccountWitness::new(dest_pk, dest_balance, dest_proof)
        } else {
            AccountWitness::new([0u32; 8], 0, dest_proof)
        };

        // Update dest balance
        let new_balance = dest_balance + deposit_amount;
        self.smt.upsert(dest_pk, new_balance);

        Some(ActionWitness::Entry(EntryWitnessData { dest: dest_witness }))
    }

    fn process_exit(&mut self, payload_words: &[u32], tx: &Transaction) -> Option<ActionWitness> {
        if payload_words.len() < ActionHeader::WORDS + ExitAction::WORDS {
            return None;
        }

        let action = ExitAction::from_words(payload_words[ActionHeader::WORDS..][..ExitAction::WORDS].try_into().unwrap());

        let source_pk = action.source;
        let exit_amount = action.amount;

        // Check source balance
        let source_balance = self.smt.get(&source_pk)?;
        if source_balance < exit_amount {
            return None; // Insufficient balance
        }

        // Build source witness
        let source_proof = self.smt.prove(&source_pk);
        let source_witness = AccountWitness::new(source_pk, source_balance, source_proof);

        // Update source balance
        let new_balance = source_balance - exit_amount;
        self.smt.upsert(source_pk, new_balance);

        // Add to permission tree (destination_spk is [u32; 10], first 35 bytes are the SPK)
        let dest_spk_bytes: &[u8] = bytemuck::cast_slice(&action.destination_spk);
        let spk_len = 35.min(dest_spk_bytes.len());
        let leaf = perm_leaf_hash(&dest_spk_bytes[..spk_len], exit_amount);
        self.perm_builder.add_leaf(leaf);
        self.accumulated_perm_builder.add_leaf(leaf);
        self.accumulated_exit_data.push((dest_spk_bytes[..spk_len].to_vec(), exit_amount));

        // Build prev_tx
        let prev_tx = build_prev_tx_for_action(tx, &source_pk);

        Some(ActionWitness::Exit(Box::new(ExitWitnessData { source: source_witness, prev_tx, prev_output_index: 0 })))
    }
}

/// Convert an RpcOptionalTransaction (from VCCv2 with High verbosity) to a consensus Transaction.
fn rpc_optional_to_transaction(rpc: &kaspa_rpc_core::RpcOptionalTransaction) -> Option<Transaction> {
    let version = rpc.version?;
    let lock_time = rpc.lock_time.unwrap_or(0);
    let subnetwork_id = rpc.subnetwork_id.unwrap_or(SUBNETWORK_ID_NATIVE);
    let gas = rpc.gas.unwrap_or(0);
    let payload = rpc.payload.clone().unwrap_or_default();

    let inputs: Vec<TransactionInput> = rpc
        .inputs
        .iter()
        .filter_map(|inp| {
            let outpoint = inp.previous_outpoint.as_ref()?;
            let tx_id = outpoint.transaction_id?;
            let index = outpoint.index?;
            Some(TransactionInput::new(
                TransactionOutpoint::new(tx_id, index),
                inp.signature_script.clone().unwrap_or_default(),
                inp.sequence.unwrap_or(0),
                inp.sig_op_count.unwrap_or(0),
            ))
        })
        .collect();

    let outputs: Vec<TransactionOutput> = rpc
        .outputs
        .iter()
        .filter_map(|out| {
            let value = out.value?;
            let spk = out.script_public_key.clone()?;
            Some(TransactionOutput::new(value, spk))
        })
        .collect();

    Some(Transaction::new(version, inputs, outputs, lock_time, subnetwork_id, gas, payload))
}

/// Build a synthetic prev_tx for action auth verification.
///
/// In a real scenario, we'd fetch the actual previous transaction.
/// For now, we create a minimal V1 tx with the source account's P2PK SPK.
fn build_prev_tx_for_action(_tx: &Transaction, source_pk: &[u32; 8]) -> Transaction {
    let source_spk_bytes = pay_to_pubkey_spk(&bytemuck::cast::<[u32; 8], [u8; 32]>(*source_pk));
    let spk = ScriptPublicKey::new(0, source_spk_bytes.to_vec().into());

    // Create a minimal V1 transaction that has an output with the source's SPK
    Transaction::new(1, vec![], vec![TransactionOutput::new(1000, spk)], 0, SUBNETWORK_ID_NATIVE, 0, vec![])
}
