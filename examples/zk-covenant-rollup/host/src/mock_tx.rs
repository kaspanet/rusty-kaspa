use kaspa_consensus_core::{
    hashing::tx::{payload_digest, transaction_v1_rest_preimage},
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
};
use kaspa_hashes::Hash;
use zk_covenant_rollup_core::{
    action::{ActionHeader, EntryAction, ExitAction, TransferAction, OP_ENTRY, OP_EXIT, OP_TRANSFER},
    bytes_to_words_ref, is_action_tx_id,
    prev_tx::PrevTxV1Witness,
    state::AccountWitness,
    AlignedBytes,
};

/// Transfer payload with header (for computing tx_id)
#[derive(Clone, Copy, Debug)]
pub struct TransferPayload {
    pub header: ActionHeader,
    pub transfer: TransferAction,
}

impl TransferPayload {
    /// Create a new transfer payload
    pub fn new(source: [u32; 8], destination: [u32; 8], amount: u64, nonce: u32) -> Self {
        Self { header: ActionHeader::new(OP_TRANSFER, nonce), transfer: TransferAction::new(source, destination, amount) }
    }

    /// Get as words for hashing
    pub fn as_words(&self) -> Vec<u32> {
        let mut words = Vec::with_capacity(ActionHeader::WORDS + TransferAction::WORDS);
        words.extend_from_slice(self.header.as_words());
        words.extend_from_slice(self.transfer.as_words());
        words
    }

    /// Get as bytes for payload field
    pub fn as_bytes(&self) -> Vec<u8> {
        bytemuck::cast_slice(&self.as_words()).to_vec()
    }
}

/// Entry (deposit) payload with header (for computing tx_id)
#[derive(Clone, Copy, Debug)]
pub struct EntryPayload {
    pub header: ActionHeader,
    pub entry: EntryAction,
}

impl EntryPayload {
    /// Create a new entry payload
    pub fn new(destination: [u32; 8], nonce: u32) -> Self {
        Self { header: ActionHeader::new(OP_ENTRY, nonce), entry: EntryAction::new(destination) }
    }

    /// Get as words for hashing
    pub fn as_words(&self) -> Vec<u32> {
        let mut words = Vec::with_capacity(ActionHeader::WORDS + EntryAction::WORDS);
        words.extend_from_slice(self.header.as_words());
        words.extend_from_slice(self.entry.as_words());
        words
    }

    /// Get as bytes for payload field
    pub fn as_bytes(&self) -> Vec<u8> {
        bytemuck::cast_slice(&self.as_words()).to_vec()
    }
}

/// Exit (withdrawal) payload with header (for computing tx_id)
#[derive(Clone, Debug)]
pub struct ExitPayload {
    pub header: ActionHeader,
    pub exit: ExitAction,
}

impl ExitPayload {
    /// Create a new exit payload
    pub fn new(source: [u32; 8], destination_spk: &[u8], amount: u64, nonce: u32) -> Self {
        Self { header: ActionHeader::new(OP_EXIT, nonce), exit: ExitAction::new(source, destination_spk, amount) }
    }

    /// Get as words for hashing
    pub fn as_words(&self) -> Vec<u32> {
        let mut words = Vec::with_capacity(ActionHeader::WORDS + ExitAction::WORDS);
        words.extend_from_slice(self.header.as_words());
        words.extend_from_slice(self.exit.as_words());
        words
    }

    /// Get as bytes for payload field
    pub fn as_bytes(&self) -> Vec<u8> {
        bytemuck::cast_slice(&self.as_words()).to_vec()
    }
}

/// Witness data for transfer actions
#[derive(Clone, Debug)]
pub struct TransferWitnessData {
    /// Source account witness
    pub source: AccountWitness,
    /// Destination account witness
    pub dest: AccountWitness,
    /// Previous transaction (the UTXO being spent)
    pub prev_tx: Transaction,
    /// Output index in the previous transaction
    pub prev_output_index: u32,
}

/// Witness data for entry (deposit) actions.
/// Deposit amount is always taken from output index 0.
/// rest_preimage is no longer needed here — it comes from V1TxData.
#[derive(Clone, Debug)]
pub struct EntryWitnessData {
    /// Destination account witness
    pub dest: AccountWitness,
}

/// Witness data for exit (withdrawal) actions.
#[derive(Clone, Debug)]
pub struct ExitWitnessData {
    /// Source account witness
    pub source: AccountWitness,
    /// Previous transaction (the UTXO being spent, proves source ownership)
    pub prev_tx: Transaction,
    /// Output index in the previous transaction
    pub prev_output_index: u32,
}

/// Witness data for action transactions (discriminated by action type)
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ActionWitness {
    Transfer(Box<TransferWitnessData>),
    Entry(EntryWitnessData),
    Exit(Box<ExitWitnessData>),
}

/// Transaction wrapper that combines a real Kaspa Transaction with ZK witness data
#[derive(Clone, Debug)]
pub struct ZkTransaction {
    /// The real Kaspa transaction
    pub tx: Transaction,
    /// Optional witness data for action transactions
    pub witness: Option<ActionWitness>,
}

impl ZkTransaction {
    /// Create a new ZkTransaction
    pub fn new(tx: Transaction, witness: Option<ActionWitness>) -> Self {
        Self { tx, witness }
    }

    /// Get the transaction version
    pub fn version(&self) -> u16 {
        self.tx.version
    }

    /// Get the transaction ID as [u32; 8]
    pub fn tx_id(&self) -> [u32; 8] {
        bytes_to_words_ref(&self.tx.id().as_bytes())
    }

    /// Write to executor env in the format expected by guest
    pub fn write_to_env(&self, builder: &mut risc0_zkvm::ExecutorEnvBuilder<'_>) {
        builder.write_slice(&(self.version() as u32).to_le_bytes());

        if self.tx.version == 0 {
            // V0: just write the tx_id
            let tx_id = self.tx_id();
            builder.write_slice(bytemuck::cast_slice::<_, u8>(&tx_id));
        } else {
            // V1+: write payload, rest_preimage (length-prefixed), and witness data if action tx
            let payload_bytes = &self.tx.payload;
            builder.write_slice(&(payload_bytes.len() as u32).to_le_bytes());
            if !payload_bytes.is_empty() {
                // Pad to word boundary
                let padded_len = payload_bytes.len().div_ceil(4) * 4;
                let mut padded = vec![0u8; padded_len];
                padded[..payload_bytes.len()].copy_from_slice(payload_bytes);
                builder.write_slice(&padded);
            }

            // Write full rest_preimage (length-prefixed) — guest computes rest_digest from it
            let rest_preimage = transaction_v1_rest_preimage(&self.tx);
            write_bytes(builder, &rest_preimage);

            // Check if this is a valid action tx and write witness data accordingly
            let tx_id = self.tx_id();
            if is_action_tx_id(&tx_id) {
                let payload_words: Vec<u32> =
                    payload_bytes.chunks_exact(4).map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap())).collect();

                match &self.witness {
                    Some(ActionWitness::Transfer(w)) if is_valid_transfer_payload(&payload_words) => {
                        // Write transfer witness: source, dest, prev_tx_v1_witness
                        // (prev_tx_id and output_index are NOT sent — guest derives them
                        //  from the current tx's first input outpoint)
                        builder.write_slice(w.source.as_bytes());
                        builder.write_slice(w.dest.as_bytes());

                        let prev_tx_witness = create_prev_tx_v1_witness(&w.prev_tx, w.prev_output_index);
                        write_prev_tx_v1_witness(builder, &prev_tx_witness);
                    }
                    Some(ActionWitness::Entry(w)) if is_valid_entry_payload(&payload_words) => {
                        // Write entry witness: dest account only
                        // (rest_preimage is already sent above for all V1 txs)
                        builder.write_slice(w.dest.as_bytes());
                    }
                    Some(ActionWitness::Exit(w)) if is_valid_exit_payload(&payload_words) => {
                        // Write exit witness: source account, prev_tx_v1_witness
                        // (prev_tx_id and output_index are NOT sent — guest derives them)
                        builder.write_slice(w.source.as_bytes());

                        let prev_tx_witness = create_prev_tx_v1_witness(&w.prev_tx, w.prev_output_index);
                        write_prev_tx_v1_witness(builder, &prev_tx_witness);
                    }
                    _ => {
                        // Panic only if this is a *known* action type — the host must always
                        // supply a witness for Transfer/Entry/Exit.  A tx with an unrecognised
                        // opcode simply gets no witness written (guest skips it).
                        let action_type = if is_valid_transfer_payload(&payload_words) {
                            Some("Transfer")
                        } else if is_valid_entry_payload(&payload_words) {
                            Some("Entry")
                        } else if is_valid_exit_payload(&payload_words) {
                            Some("Exit")
                        } else {
                            None
                        };
                        if let Some(action_type) = action_type {
                            let tx_hash = self.tx.id();
                            let prev_info = self.tx.inputs.first().map_or_else(
                                || "no inputs".to_string(),
                                |inp| format!(
                                    "needs prev_tx={} output_idx={}",
                                    inp.previous_outpoint.transaction_id,
                                    inp.previous_outpoint.index
                                ),
                            );
                            let witness_desc = match &self.witness {
                                None => "witness=None (prev_tx not found in DB)",
                                Some(ActionWitness::Transfer(_)) => "wrong type: have Transfer",
                                Some(ActionWitness::Entry(_)) => "wrong type: have Entry",
                                Some(ActionWitness::Exit(_)) => "wrong type: have Exit",
                            };
                            panic!(
                                "host has no witness for {} action tx {} | {} | {}",
                                action_type, tx_hash, prev_info, witness_desc
                            );
                        }
                        // Unknown opcode — write nothing; guest will skip.
                    }
                }
            }
        }
    }
}

/// Check if payload words represent a valid transfer payload
fn is_valid_transfer_payload(payload_words: &[u32]) -> bool {
    if payload_words.len() < ActionHeader::WORDS + TransferAction::WORDS {
        return false;
    }
    let header = ActionHeader::from_words_ref(payload_words[..ActionHeader::WORDS].try_into().unwrap());
    if !header.is_valid_version() || header.operation != OP_TRANSFER {
        return false;
    }
    let transfer = TransferAction::from_words(payload_words[ActionHeader::WORDS..][..TransferAction::WORDS].try_into().unwrap());
    transfer.is_valid()
}

/// Check if payload words represent a valid entry payload
fn is_valid_entry_payload(payload_words: &[u32]) -> bool {
    if payload_words.len() < ActionHeader::WORDS + EntryAction::WORDS {
        return false;
    }
    let header = ActionHeader::from_words_ref(payload_words[..ActionHeader::WORDS].try_into().unwrap());
    if !header.is_valid_version() || header.operation != OP_ENTRY {
        return false;
    }
    let entry = EntryAction::from_words(payload_words[ActionHeader::WORDS..][..EntryAction::WORDS].try_into().unwrap());
    entry.is_valid()
}

/// Create a PrevTxV1Witness from a real Transaction
fn create_prev_tx_v1_witness(prev_tx: &Transaction, output_index: u32) -> PrevTxV1Witness {
    assert!(prev_tx.version >= 1, "PrevTxV1Witness requires V1+ transaction");

    let rest_preimage = transaction_v1_rest_preimage(prev_tx);
    let pd = payload_digest(&prev_tx.payload);
    let payload_digest_words = bytes_to_words_ref(&pd.as_bytes());

    PrevTxV1Witness::new(output_index, AlignedBytes::from_bytes(&rest_preimage), payload_digest_words)
}

/// Write PrevTxV1Witness to executor env.
///
/// Does NOT write prev_tx_id or output_index — the guest derives those from
/// the current action tx's first input outpoint (committed in rest_preimage).
fn write_prev_tx_v1_witness(builder: &mut risc0_zkvm::ExecutorEnvBuilder<'_>, witness: &PrevTxV1Witness) {
    // Write rest_preimage with length prefix
    write_bytes(builder, witness.rest_preimage.as_bytes());

    // Write payload_digest (fixed 32 bytes, no length prefix needed)
    builder.write_slice(bytemuck::cast_slice::<_, u8>(&witness.payload_digest));
}

/// Write length-prefixed bytes to executor env
fn write_bytes(builder: &mut risc0_zkvm::ExecutorEnvBuilder<'_>, data: &[u8]) {
    // Write length as u64
    builder.write_slice(&(data.len() as u64).to_le_bytes());

    if !data.is_empty() {
        // Pad to word boundary
        let padded_len = data.len().div_ceil(4) * 4;
        let mut padded = vec![0u8; padded_len];
        padded[..data.len()].copy_from_slice(data);
        builder.write_slice(&padded);
    }
}

/// Find a nonce that makes the tx_id start with ACTION_TX_ID_PREFIX (for transfers)
pub fn find_action_tx_nonce(
    source: [u32; 8],
    destination: [u32; 8],
    amount: u64,
    inputs: &[TransactionInput],
    outputs: &[TransactionOutput],
) -> TransferPayload {
    for nonce in 0u32.. {
        let payload = TransferPayload::new(source, destination, amount, nonce);

        let tx = Transaction::new(1, inputs.to_vec(), outputs.to_vec(), 0, SUBNETWORK_ID_NATIVE, 0, payload.as_bytes());

        let tx_id = bytes_to_words_ref(&tx.id().as_bytes());
        if is_action_tx_id(&tx_id) {
            return payload;
        }
    }
    unreachable!()
}

/// Find a nonce that makes the tx_id start with ACTION_TX_ID_PREFIX (for entry/deposits)
pub fn find_entry_tx_nonce(destination: [u32; 8], inputs: &[TransactionInput], outputs: &[TransactionOutput]) -> EntryPayload {
    for nonce in 0u32.. {
        let payload = EntryPayload::new(destination, nonce);

        let tx = Transaction::new(1, inputs.to_vec(), outputs.to_vec(), 0, SUBNETWORK_ID_NATIVE, 0, payload.as_bytes());

        let tx_id = bytes_to_words_ref(&tx.id().as_bytes());
        if is_action_tx_id(&tx_id) {
            return payload;
        }
    }
    unreachable!()
}

/// Create a V0 transaction (non-action)
pub fn create_v0_tx(tx_id_bytes: [u32; 8]) -> ZkTransaction {
    let tx_id_hash = Hash::from_bytes(bytemuck::cast(tx_id_bytes));
    let tx = Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(tx_id_hash, 0), vec![], 0, 0)],
        vec![],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    ZkTransaction::new(tx, None)
}

/// Check if payload words represent a valid exit payload
fn is_valid_exit_payload(payload_words: &[u32]) -> bool {
    if payload_words.len() < ActionHeader::WORDS + ExitAction::WORDS {
        return false;
    }
    let header = ActionHeader::from_words_ref(payload_words[..ActionHeader::WORDS].try_into().unwrap());
    if !header.is_valid_version() || header.operation != OP_EXIT {
        return false;
    }
    let exit = ExitAction::from_words(payload_words[ActionHeader::WORDS..][..ExitAction::WORDS].try_into().unwrap());
    exit.is_valid()
}

/// Find a nonce that makes the tx_id start with ACTION_TX_ID_PREFIX (for exit/withdrawals)
pub fn find_exit_tx_nonce(
    source: [u32; 8],
    destination_spk: &[u8],
    amount: u64,
    inputs: &[TransactionInput],
    outputs: &[TransactionOutput],
) -> ExitPayload {
    for nonce in 0u32.. {
        let payload = ExitPayload::new(source, destination_spk, amount, nonce);

        let tx = Transaction::new(1, inputs.to_vec(), outputs.to_vec(), 0, SUBNETWORK_ID_NATIVE, 0, payload.as_bytes());

        let tx_id = bytes_to_words_ref(&tx.id().as_bytes());
        if is_action_tx_id(&tx_id) {
            return payload;
        }
    }
    unreachable!()
}

/// Create a V1 exit (withdrawal) action transaction with witness data.
pub fn create_exit_tx(
    source: [u32; 8],
    destination_spk: &[u8],
    amount: u64,
    outputs: Vec<TransactionOutput>,
    source_witness: AccountWitness,
    prev_tx: Transaction,
    prev_output_index: u32,
) -> ZkTransaction {
    let input = TransactionInput::new(TransactionOutpoint::new(prev_tx.id(), prev_output_index), vec![], 0, 0);

    let payload = find_exit_tx_nonce(source, destination_spk, amount, std::slice::from_ref(&input), &outputs);

    let tx = Transaction::new(1, vec![input], outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload.as_bytes());

    ZkTransaction::new(tx, Some(ActionWitness::Exit(Box::new(ExitWitnessData { source: source_witness, prev_tx, prev_output_index }))))
}

/// Create a "previous transaction" for use as UTXO source.
/// This creates a V1 transaction with a single output containing the given SPK.
pub fn create_prev_tx(output_value: u64, output_spk: ScriptPublicKey) -> Transaction {
    Transaction::new(
        1,
        vec![], // No inputs needed for prev tx in testing
        vec![TransactionOutput::new(output_value, output_spk)],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![], // Empty payload for prev tx
    )
}

/// Create a V1 transfer action transaction with witness data
pub fn create_transfer_tx(
    source: [u32; 8],
    destination: [u32; 8],
    amount: u64,
    outputs: Vec<TransactionOutput>,
    source_witness: AccountWitness,
    dest_witness: AccountWitness,
    prev_tx: Transaction,
    prev_output_index: u32,
) -> ZkTransaction {
    let input = TransactionInput::new(TransactionOutpoint::new(prev_tx.id(), prev_output_index), vec![], 0, 0);

    // Find nonce that makes tx_id an action (with the correct input)
    let payload = find_action_tx_nonce(source, destination, amount, std::slice::from_ref(&input), &outputs);

    // Create the actual transaction
    let tx = Transaction::new(1, vec![input], outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload.as_bytes());

    ZkTransaction::new(
        tx,
        Some(ActionWitness::Transfer(Box::new(TransferWitnessData {
            source: source_witness,
            dest: dest_witness,
            prev_tx,
            prev_output_index,
        }))),
    )
}

/// Create a V1 entry (deposit) action transaction with witness data.
/// The deposit amount is always taken from output index 0.
pub fn create_entry_tx(destination: [u32; 8], outputs: Vec<TransactionOutput>, dest_witness: AccountWitness) -> ZkTransaction {
    // Entry txs don't need a specific prev tx input for authorization
    let payload = find_entry_tx_nonce(destination, &[], &outputs);

    // Create the actual transaction
    let tx = Transaction::new(1, vec![], outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload.as_bytes());

    ZkTransaction::new(tx, Some(ActionWitness::Entry(EntryWitnessData { dest: dest_witness })))
}

/// Create a V1 transaction that is NOT an action (tx_id doesn't start with action prefix).
/// This tests that the guest correctly ignores non-action V1 transactions.
pub fn create_v1_non_action_tx() -> ZkTransaction {
    // Create a simple V1 tx with arbitrary payload that won't have action prefix
    // Using empty payload ensures it won't be detected as action
    let tx = Transaction::new(
        1,
        vec![],
        vec![TransactionOutput::new(100, ScriptPublicKey::new(0, vec![0u8; 34].into()))],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![], // Empty payload - not an action
    );

    // Verify it's not an action
    let tx_id = bytes_to_words_ref(&tx.id().as_bytes());
    debug_assert!(!is_action_tx_id(&tx_id), "V1 non-action tx should not have action prefix");

    ZkTransaction::new(tx, None)
}

/// Create a V1 transaction with action prefix but UNKNOWN operation code.
/// This tests that the guest correctly rejects unknown action types.
pub fn create_unknown_action_tx() -> ZkTransaction {
    const UNKNOWN_OP: u16 = 0xFFFF; // Unknown operation code

    // Find a nonce that makes tx_id an action
    let outputs = vec![TransactionOutput::new(100, ScriptPublicKey::new(0, vec![0u8; 34].into()))];

    for nonce in 0u32.. {
        let header = ActionHeader { version: zk_covenant_rollup_core::action::ACTION_VERSION, operation: UNKNOWN_OP, nonce };
        let payload_bytes: Vec<u8> = bytemuck::cast_slice(header.as_words()).to_vec();

        let tx = Transaction::new(1, vec![], outputs.clone(), 0, SUBNETWORK_ID_NATIVE, 0, payload_bytes);

        let tx_id = bytes_to_words_ref(&tx.id().as_bytes());
        if is_action_tx_id(&tx_id) {
            return ZkTransaction::new(tx, None);
        }
    }
    unreachable!()
}
