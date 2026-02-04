use risc0_zkvm::serde::WordRead;
use zk_covenant_rollup_core::{
    prev_tx::{verify_output_in_tx, PrevTxV1Witness, PrevTxWitness},
    state::AccountWitness,
    OutputData,
};

use crate::input;

/// Witness data for verifying a previous V1 transaction output
pub struct PrevTxV1WitnessData {
    /// The previous transaction ID being spent
    pub prev_tx_id: [u32; 8],
    /// V1 witness with rest_preimage and payload_digest
    pub witness: PrevTxV1Witness,
}

impl PrevTxV1WitnessData {
    pub fn read_from_stdin(stdin: &mut impl WordRead) -> Self {
        let prev_tx_id = input::read_hash(stdin);
        let witness = Self::read_v1_witness(stdin);
        Self { prev_tx_id, witness }
    }

    fn read_v1_witness(stdin: &mut impl WordRead) -> PrevTxV1Witness {
        let output_index = input::read_u32(stdin);
        let rest_preimage = input::read_aligned_bytes(stdin);
        let payload_digest = input::read_hash(stdin);

        PrevTxV1Witness::new(output_index, rest_preimage, payload_digest)
    }

    /// Verify the output SPK is committed to the prev_tx_id
    /// Returns the full output data if verification succeeds
    pub fn verify_output(&self) -> Option<OutputData> {
        let wrapped = PrevTxWitness::V1(self.witness.clone());
        verify_output_in_tx(&self.prev_tx_id, &wrapped)
    }
}

/// Complete witness data for a transfer action
pub struct TransferWitness {
    /// Source account SMT witness
    pub source: AccountWitness,
    /// Destination account SMT witness
    pub dest: AccountWitness,
    /// Previous tx output witness (proves source ownership)
    pub prev_tx: PrevTxV1WitnessData,
}

impl TransferWitness {
    pub fn read_from_stdin(stdin: &mut impl WordRead) -> Self {
        Self {
            source: input::read_account_witness(stdin),
            dest: input::read_account_witness(stdin),
            prev_tx: PrevTxV1WitnessData::read_from_stdin(stdin),
        }
    }
}
