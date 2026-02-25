use risc0_zkvm::serde::WordRead;
use zk_covenant_rollup_core::{prev_tx::PrevTxV1Witness, state::AccountWitness};

use crate::input;

/// Witness for a previous V1 transaction (proves output content).
///
/// Does NOT include prev_tx_id or output_index — those are derived from
/// the current action transaction's first input outpoint (committed via rest_preimage).
/// This prevents the host from substituting a fake prev_tx.
pub struct PrevTxV1WitnessData {
    /// V1 witness with rest_preimage and payload_digest
    pub witness: PrevTxV1Witness,
}

impl PrevTxV1WitnessData {
    /// Read from stdin. The prev_tx_id and output_index are NOT read here —
    /// they come from the current tx's first input (parsed from rest_preimage).
    pub fn read_from_stdin(stdin: &mut impl WordRead, output_index: u32) -> Self {
        let witness = Self::read_v1_witness(stdin, output_index);
        Self { witness }
    }

    fn read_v1_witness(stdin: &mut impl WordRead, output_index: u32) -> PrevTxV1Witness {
        let rest_preimage = input::read_aligned_bytes(stdin);
        let payload_digest = input::read_hash(stdin);

        PrevTxV1Witness::new(output_index, rest_preimage, payload_digest)
    }
}

// ANCHOR: transfer_witness
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
    pub fn read_from_stdin(stdin: &mut impl WordRead, output_index: u32) -> Self {
        Self {
            source: input::read_account_witness(stdin),
            dest: input::read_account_witness(stdin),
            prev_tx: PrevTxV1WitnessData::read_from_stdin(stdin, output_index),
        }
    }
}
// ANCHOR_END: transfer_witness

// ANCHOR: exit_witness
/// Witness data for an exit (withdrawal) action.
///
/// Exits debit the source account and create a permission tree leaf.
/// Similar to transfer: requires source authorization via prev tx output.
pub struct ExitWitness {
    /// Source account SMT witness
    pub source: AccountWitness,
    /// Previous tx output witness (proves source ownership)
    pub prev_tx: PrevTxV1WitnessData,
}

impl ExitWitness {
    pub fn read_from_stdin(stdin: &mut impl WordRead, output_index: u32) -> Self {
        Self { source: input::read_account_witness(stdin), prev_tx: PrevTxV1WitnessData::read_from_stdin(stdin, output_index) }
    }
}
// ANCHOR_END: exit_witness

// ANCHOR: entry_witness
/// Witness data for an entry (deposit) action.
///
/// Entry deposits don't need source authorization (no source account).
/// The rest_preimage (for extracting deposit amount from output 0) is now
/// provided via V1TxData — no longer part of this witness.
pub struct EntryWitness {
    /// Destination account SMT proof (for crediting the deposit)
    pub dest: AccountWitness,
}

impl EntryWitness {
    pub fn read_from_stdin(stdin: &mut impl WordRead) -> Self {
        Self { dest: input::read_account_witness(stdin) }
    }
}
// ANCHOR_END: entry_witness
