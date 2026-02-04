use zk_covenant_rollup_core::{
    action::{ActionHeader, TransferAction, OP_TRANSFER},
    is_action_tx_id, payload_digest, payload_digest_bytes,
    prev_tx::PrevTxV1Witness,
    rest_digest,
    state::AccountWitness,
    tx_id_v1, AlignedBytes,
};

/// "Other data" that goes into rest_digest.
/// In a real implementation, this would include outputs, locktime, etc.
/// Note: rest_digest does NOT include input SPKs (kaspa-compatible).
pub const OTHER_DATA_WORDS: usize = 8;
pub type OtherData = [u32; OTHER_DATA_WORDS];

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

    /// Check if the transfer is valid
    pub fn is_valid(&self) -> bool {
        self.header.is_valid_version() && self.transfer.is_valid()
    }
}

/// Represents a mock transaction to be included in a block
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum MockTx {
    /// Version 0 tx: just a raw tx_id (no payload processing)
    V0 { tx_id: [u32; 8] },
    /// Version 1+ tx: has payload, rest_digest components, and optional witness data
    V1 {
        version: u16,
        payload: TransferPayload,
        /// The "other data" portion of rest_digest (outputs, locktime, etc.)
        /// rest_digest = hash(other_data) - kaspa-compatible (no input SPKs)
        other_data: OtherData,
        /// Witness data for valid action transactions (None if not an action or invalid)
        witness: Option<ActionWitnessData>,
    },
}

/// Witness data for action transactions
#[derive(Clone, Debug)]
pub struct ActionWitnessData {
    /// Source account witness
    pub source: AccountWitness,
    /// Destination account witness
    pub dest: AccountWitness,
    /// Previous transaction ID (UTXO being spent)
    pub prev_tx_id: [u32; 8],
    /// V1 witness with rest_preimage and payload_digest for verifying the prev tx output
    pub prev_tx_witness: PrevTxV1Witness,
}

impl MockTx {
    pub fn version(&self) -> u16 {
        match self {
            MockTx::V0 { .. } => 0,
            MockTx::V1 { version, .. } => *version,
        }
    }

    pub fn tx_id(&self) -> [u32; 8] {
        match self {
            MockTx::V0 { tx_id } => *tx_id,
            MockTx::V1 { payload, other_data, .. } => {
                let pd = payload_digest(&payload.as_words());
                // rest_digest is computed from other_data only (kaspa-compatible)
                // Source is committed via payload_digest (payload contains source)
                let rd = rest_digest(other_data);
                tx_id_v1(&pd, &rd)
            }
        }
    }

    /// Write to executor env in the format expected by guest
    pub fn write_to_env(&self, builder: &mut risc0_zkvm::ExecutorEnvBuilder<'_>) {
        builder.write_slice(&(self.version() as u32).to_le_bytes());
        match self {
            MockTx::V0 { tx_id } => {
                builder.write_slice(bytemuck::cast_slice::<_, u8>(tx_id));
            }
            MockTx::V1 { payload, other_data, witness, .. } => {
                // Write payload length in BYTES then payload (word-aligned)
                let payload_words = payload.as_words();
                let payload_bytes: &[u8] = bytemuck::cast_slice(&payload_words);
                builder.write_slice(&(payload_bytes.len() as u32).to_le_bytes());
                builder.write_slice(payload_bytes);
                // Write rest_digest directly (pre-computed from arbitrary-length rest data)
                let rd = rest_digest(other_data);
                builder.write_slice(bytemuck::cast_slice::<_, u8>(&rd));

                // Guest determines if this is an action based on tx_id + payload validity.
                // If it IS a valid action, guest will expect witness data - we must provide it.
                // No flag needed - the guest's decision is deterministic from the data.
                let tx_id = self.tx_id();
                let is_action = is_action_tx_id(&tx_id) && payload.is_valid();

                if is_action {
                    // Guest will read witness data - we must provide it
                    let w = witness.as_ref().expect("Valid action tx must have witness data");
                    // Write source account witness
                    builder.write_slice(w.source.as_bytes());
                    // Write dest account witness
                    builder.write_slice(w.dest.as_bytes());
                    // Write prev_tx_id
                    builder.write_slice(bytemuck::cast_slice::<_, u8>(&w.prev_tx_id));
                    // Write prev tx V1 witness
                    write_prev_tx_v1_witness(builder, &w.prev_tx_witness);
                }
                // For non-action txs, guest won't read anything more - don't write anything
            }
        }
    }
}

/// Write PrevTxV1Witness to executor env
fn write_prev_tx_v1_witness(builder: &mut risc0_zkvm::ExecutorEnvBuilder<'_>, witness: &PrevTxV1Witness) {
    // Write output_index
    builder.write_slice(&witness.output_index.to_le_bytes());

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

/// Find a nonce that makes the tx_id start with ACTION_TX_ID_PREFIX (single byte)
pub fn find_action_tx_nonce(source: [u32; 8], destination: [u32; 8], amount: u64, other_data: &OtherData) -> TransferPayload {
    for nonce in 0u32.. {
        let payload = TransferPayload::new(source, destination, amount, nonce);
        let pd = payload_digest(&payload.as_words());
        let rd = rest_digest(other_data);
        let tx_id = tx_id_v1(&pd, &rd);
        if is_action_tx_id(&tx_id) {
            println!("  Found valid action nonce: {}", nonce);
            return payload;
        }
    }
    unreachable!()
}

/// Create a V0 transaction (non-action)
pub fn create_v0_tx(tx_id: [u32; 8]) -> MockTx {
    MockTx::V0 { tx_id }
}

/// Build a V1 rest_preimage (full transaction without payload) for testing
fn build_v1_rest_preimage(output_value: u64, output_spk: &[u8; 34]) -> Vec<u8> {
    let mut rest = Vec::new();
    // version
    rest.extend_from_slice(&1u16.to_le_bytes());
    // 0 inputs
    rest.extend_from_slice(&0u64.to_le_bytes());
    // 1 output
    rest.extend_from_slice(&1u64.to_le_bytes());
    // output: value
    rest.extend_from_slice(&output_value.to_le_bytes());
    // output: spk_version
    rest.extend_from_slice(&0u16.to_le_bytes());
    // output: spk_len
    rest.extend_from_slice(&34u64.to_le_bytes());
    // output: spk
    rest.extend_from_slice(output_spk);
    // output: has_covenant = false
    rest.push(0);
    // locktime
    rest.extend_from_slice(&0u64.to_le_bytes());
    // subnetwork_id
    rest.extend_from_slice(&[0u8; 20]);
    // gas
    rest.extend_from_slice(&0u64.to_le_bytes());
    // empty_payload_len
    rest.extend_from_slice(&0u64.to_le_bytes());
    // mass
    rest.extend_from_slice(&0u64.to_le_bytes());
    rest
}

/// Create a mock V1 "previous transaction" with the given output.
///
/// The mock transaction has:
/// - Version 1
/// - 0 inputs
/// - 1 output at the specified index
/// - All other fields zeroed
///
/// Returns (prev_tx_id, prev_tx_v1_witness) for use in action transaction verification.
fn create_mock_prev_tx_v1(output_value: u64, output_spk: [u8; 34], output_index: u32) -> ([u32; 8], PrevTxV1Witness) {
    // Build full rest_preimage
    let rest_preimage = build_v1_rest_preimage(output_value, &output_spk);

    // Empty payload for the mock prev tx - compute its digest
    let payload_digest = payload_digest_bytes(&[]);

    // Create the V1 witness
    let witness = PrevTxV1Witness::new(output_index, AlignedBytes::from_bytes(&rest_preimage), payload_digest);

    // Compute tx_id from the witness
    let tx_id = witness.compute_tx_id();

    (tx_id, witness)
}

/// Create a V1 transfer action transaction with witness data
///
/// The source pubkey is included in the payload and committed to tx_id via payload_digest.
/// A mock previous transaction is created to provide cryptographic proof of source ownership.
pub fn create_transfer_tx(
    source: [u32; 8],
    destination: [u32; 8],
    amount: u64,
    other_data: OtherData,
    source_witness: AccountWitness,
    dest_witness: AccountWitness,
    first_input_spk: [u8; 34],
) -> MockTx {
    // Create a mock previous transaction that has an output with source's SPK
    // This simulates the UTXO being spent
    let (prev_tx_id, prev_tx_witness) = create_mock_prev_tx_v1(
        1000, // arbitrary output value
        first_input_spk,
        0, // output index
    );

    // Find nonce that makes tx_id an action
    let payload = find_action_tx_nonce(source, destination, amount, &other_data);

    MockTx::V1 {
        version: 1,
        payload,
        other_data,
        witness: Some(ActionWitnessData { source: source_witness, dest: dest_witness, prev_tx_id, prev_tx_witness }),
    }
}
