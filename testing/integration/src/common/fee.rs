use kaspa_consensus_core::{
    constants::{STORAGE_MASS_PARAMETER, TRANSIENT_BYTE_TO_MASS_FACTOR},
    mass::MassCalculator,
    tx::Transaction,
};

// Minimum standard relay fee is 100 sompi/gram; use 101 for fixture slack.
pub(crate) const FEE_RATE: u64 = 101;

// Mempool minimum relay fee uses post-Toccata cofactors immediately, so we normalize accordingly (factor / ratio between limits).
const NORMALIZED_TRANSIENT_BYTE_FACTOR: u64 = 2;

/// Calculates the relay fee used by integration fixtures for plain vanilla std txs.
pub const fn calc_for_plain_standard_tx(num_inputs: usize, num_outputs: u64) -> u64 {
    calc_for_plain_standard_tx_with_extra_serialized_bytes(num_inputs, num_outputs, 0)
}

/// Calculates the relay fee used by integration fixtures for plain vanilla std txs,
/// with known extra serialized bytes such as a larger signature script.
pub const fn calc_for_plain_standard_tx_with_extra_serialized_bytes(
    num_inputs: usize,
    num_outputs: u64,
    extra_serialized_bytes: u64,
) -> u64 {
    let (compute_mass, serialized_bytes) =
        estimated_plain_standard_tx_compute_mass_and_serialized_bytes(num_inputs, num_outputs, extra_serialized_bytes);
    let normalized_transient_mass = serialized_bytes * NORMALIZED_TRANSIENT_BYTE_FACTOR;
    let fee_mass = if compute_mass > normalized_transient_mass { compute_mass } else { normalized_transient_mass };
    FEE_RATE * fee_mass
}

/// Calculates relay fee from a transaction probe using the real non-contextual mass calculator.
pub fn calc_for_transaction(tx: &Transaction) -> u64 {
    let masses = MassCalculator::new(1, 10, STORAGE_MASS_PARAMETER).calc_non_contextual_masses(tx);
    let serialized_bytes = masses.transient_mass / TRANSIENT_BYTE_TO_MASS_FACTOR;
    let normalized_transient_mass = serialized_bytes * NORMALIZED_TRANSIENT_BYTE_FACTOR;
    FEE_RATE * masses.compute_mass.max(normalized_transient_mass)
}

/// Builds a transaction probe and calculates its relay fee using the real non-contextual mass calculator.
pub fn calc_from_probe(build_probe: impl FnOnce() -> Transaction) -> u64 {
    calc_for_transaction(&build_probe())
}

// Estimates mass for plain vanilla std txs used by integration fixtures:
// native subnet, no payload, no covenants, standard single-signature inputs,
// and standard pay-to-address outputs. Callers with custom input mass, payload,
// covenants, or other non-plain shapes should use a transaction probe.
const fn estimated_plain_standard_tx_compute_mass_and_serialized_bytes(
    num_inputs: usize,
    num_outputs: u64,
    extra_serialized_bytes: u64,
) -> (u64, u64) {
    let serialized_bytes = 94 // plain tx: version [2] + input/output counts [16] + locktime [8] + subnetwork id [20] + gas [8] + payload hash [32] + payload length [8]
        + 118 * (num_inputs as u64) // std input: outpoint [36] + signature script length [8] + single-signature script [66] + sequence [8]
        + 53 * num_outputs // std output: value [8] + script public key version [2] + script public key length [8] + max std script public key [35]
        + extra_serialized_bytes;
    let compute_mass = serialized_bytes
        + 1000 * (num_inputs as u64) // std input script mass (1 sigop)
        + 370 * num_outputs; // std output spk mass: (script public key version [2] + max std script public key [35]) * 10
    (compute_mass, serialized_bytes)
}
