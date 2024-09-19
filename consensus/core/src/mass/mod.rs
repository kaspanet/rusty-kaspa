use crate::{
    config::params::Params,
    subnets::SUBNETWORK_ID_SIZE,
    tx::{Transaction, TransactionInput, TransactionOutput, VerifiableTransaction},
};
use kaspa_hashes::HASH_SIZE;

/// Temp enum for the transition phases of KIP9
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Kip9Version {
    /// Initial KIP9 mass calculation, w/o the relaxed formula and summing storage mass and compute mass
    Alpha,

    /// Currently proposed KIP9 mass calculation, with the relaxed formula (for the cases `|O| = 1 OR |O| <= |I| <= 2`),
    /// and using a maximum operator over storage and compute mass
    Beta,
}

// transaction_estimated_serialized_size is the estimated size of a transaction in some
// serialization. This has to be deterministic, but not necessarily accurate, since
// it's only used as the size component in the transaction and block mass limit
// calculation.
pub fn transaction_estimated_serialized_size(tx: &Transaction) -> u64 {
    let mut size: u64 = 0;
    size += 2; // Tx version (u16)
    size += 8; // Number of inputs (u64)
    let inputs_size: u64 = tx.inputs.iter().map(transaction_input_estimated_serialized_size).sum();
    size += inputs_size;

    size += 8; // number of outputs (u64)
    let outputs_size: u64 = tx.outputs.iter().map(transaction_output_estimated_serialized_size).sum();
    size += outputs_size;

    size += 8; // lock time (u64)
    size += SUBNETWORK_ID_SIZE as u64;
    size += 8; // gas (u64)
    size += HASH_SIZE as u64; // payload hash

    size += 8; // length of the payload (u64)
    size += tx.payload.len() as u64;
    size
}

fn transaction_input_estimated_serialized_size(input: &TransactionInput) -> u64 {
    let mut size = 0;
    size += outpoint_estimated_serialized_size();

    size += 8; // length of signature script (u64)
    size += input.signature_script.len() as u64;

    size += 8; // sequence (uint64)
    size
}

const fn outpoint_estimated_serialized_size() -> u64 {
    let mut size: u64 = 0;
    size += HASH_SIZE as u64; // Previous tx ID
    size += 4; // Index (u32)
    size
}

pub fn transaction_output_estimated_serialized_size(output: &TransactionOutput) -> u64 {
    let mut size: u64 = 0;
    size += 8; // value (u64)
    size += 2; // output.ScriptPublicKey.Version (u16)
    size += 8; // length of script public key (u64)
    size += output.script_public_key.script().len() as u64;
    size
}

// Note: consensus mass calculator operates on signed transactions.
// To calculate mass for unsigned transactions, please use
// `kaspa_wallet_core::tx::mass::MassCalculator`
#[derive(Clone)]
pub struct MassCalculator {
    mass_per_tx_byte: u64,
    mass_per_script_pub_key_byte: u64,
    mass_per_sig_op: u64,
    storage_mass_parameter: u64,
}

impl MassCalculator {
    pub fn new(mass_per_tx_byte: u64, mass_per_script_pub_key_byte: u64, mass_per_sig_op: u64, storage_mass_parameter: u64) -> Self {
        Self { mass_per_tx_byte, mass_per_script_pub_key_byte, mass_per_sig_op, storage_mass_parameter }
    }

    pub fn new_with_consensus_params(consensus_params: &Params) -> Self {
        Self {
            mass_per_tx_byte: consensus_params.mass_per_tx_byte,
            mass_per_script_pub_key_byte: consensus_params.mass_per_script_pub_key_byte,
            mass_per_sig_op: consensus_params.mass_per_sig_op,
            storage_mass_parameter: consensus_params.storage_mass_parameter,
        }
    }

    /// Calculates the compute mass of this transaction. This does not include the storage mass calculation below which
    /// requires full UTXO context
    pub fn calc_tx_compute_mass(&self, tx: &Transaction) -> u64 {
        if tx.is_coinbase() {
            return 0;
        }

        let size = transaction_estimated_serialized_size(tx);
        let mass_for_size = size * self.mass_per_tx_byte;
        let total_script_public_key_size: u64 = tx
            .outputs
            .iter()
            .map(|output| 2 /* script public key version (u16) */ + output.script_public_key.script().len() as u64)
            .sum();
        let total_script_public_key_mass = total_script_public_key_size * self.mass_per_script_pub_key_byte;

        let total_sigops: u64 = tx.inputs.iter().map(|input| input.sig_op_count as u64).sum();
        let total_sigops_mass = total_sigops * self.mass_per_sig_op;

        mass_for_size + total_script_public_key_mass + total_sigops_mass
    }

    /// Calculates the storage mass for this populated transaction.
    /// Assumptions which must be verified before this call:
    ///     1. All output values are non-zero
    ///     2. At least one input (unless coinbase)
    ///
    /// Otherwise this function should never fail.
    pub fn calc_tx_storage_mass(&self, tx: &impl VerifiableTransaction, version: Kip9Version) -> Option<u64> {
        calc_storage_mass(
            tx.is_coinbase(),
            tx.populated_inputs().map(|(_, entry)| entry.amount),
            tx.outputs().iter().map(|out| out.value),
            version,
            self.storage_mass_parameter,
        )
    }

    /// Calculates the overall mass of this transaction, combining both compute and storage masses.
    /// The combination strategy depends on the version passed.
    pub fn calc_tx_overall_mass(
        &self,
        tx: &impl VerifiableTransaction,
        cached_compute_mass: Option<u64>,
        version: Kip9Version,
    ) -> Option<u64> {
        match version {
            Kip9Version::Alpha => self
                .calc_tx_storage_mass(tx, version)
                .and_then(|mass| mass.checked_add(cached_compute_mass.unwrap_or_else(|| self.calc_tx_compute_mass(tx.tx())))),
            Kip9Version::Beta => self
                .calc_tx_storage_mass(tx, version)
                .map(|mass| mass.max(cached_compute_mass.unwrap_or_else(|| self.calc_tx_compute_mass(tx.tx())))),
        }
    }
}

/// Calculates the storage mass for the provided input and output values.
/// Assumptions which must be verified before this call:
///     1. All output values are non-zero
///     2. At least one input (unless coinbase)
///
/// Otherwise this function should never fail.
pub fn calc_storage_mass(
    is_coinbase: bool,
    input_values: impl ExactSizeIterator<Item = u64>,
    output_values: impl ExactSizeIterator<Item = u64>,
    version: Kip9Version,
    storage_mass_parameter: u64,
) -> Option<u64> {
    if is_coinbase {
        return Some(0);
    }

    let outs_len = output_values.len() as u64;
    let ins_len = input_values.len() as u64;

    /* The code below computes the following formula:

            max( 0 , C·( |O|/H(O) - |I|/A(I) ) )

    where C is the mass storage parameter, O is the set of output values, I is the set of
    input values, H(S) := |S|/sum_{s in S} 1 / s is the harmonic mean over the set S and
    A(S) := sum_{s in S} / |S| is the arithmetic mean.

    See KIP-0009 for more details
    */

    // Since we are doing integer division, we perform the multiplication with C over the inner
    // fractions, otherwise we'll get a sum of zeros or ones.
    //
    // If sum of fractions overflowed (nearly impossible, requires 10^7 outputs for C = 10^12),
    // we return `None` indicating mass is incomputable
    //
    // Note: in theory this can be tighten by subtracting input mass in the process (possibly avoiding the overflow),
    // however the overflow case is so unpractical with current mass limits so we avoid the hassle
    let harmonic_outs =
        output_values.map(|out| storage_mass_parameter / out).try_fold(0u64, |total, current| total.checked_add(current))?; // C·|O|/H(O)

    /*
      KIP-0009 relaxed formula for the cases |O| = 1 OR |O| <= |I| <= 2:
          max( 0 , C·( |O|/H(O) - |I|/H(I) ) )

       Note: in the case |I| = 1 both formulas are equal, yet the following code (harmonic_ins) is a bit more efficient.
             Hence, we transform the condition to |O| = 1 OR |I| = 1 OR |O| = |I| = 2 which is equivalent (and faster).
    */
    if version == Kip9Version::Beta && (outs_len == 1 || ins_len == 1 || (outs_len == 2 && ins_len == 2)) {
        let harmonic_ins =
            input_values.map(|value| storage_mass_parameter / value).fold(0u64, |total, current| total.saturating_add(current)); // C·|I|/H(I)
        return Some(harmonic_outs.saturating_sub(harmonic_ins)); // max( 0 , C·( |O|/H(O) - |I|/H(I) ) );
    }

    // Total supply is bounded, so a sum of existing UTXO entries cannot overflow (nor can it be zero)
    let sum_ins = input_values.sum::<u64>(); // |I|·A(I)
    let mean_ins = sum_ins / ins_len;

    // Inner fraction must be with C and over the mean value, in order to maximize precision.
    // We can saturate the overall expression at u64::MAX since we lower-bound the subtraction below by zero anyway
    let arithmetic_ins = ins_len.saturating_mul(storage_mass_parameter / mean_ins); // C·|I|/A(I)

    Some(harmonic_outs.saturating_sub(arithmetic_ins)) // max( 0 , C·( |O|/H(O) - |I|/A(I) ) )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        constants::{SOMPI_PER_KASPA, STORAGE_MASS_PARAMETER},
        subnets::SubnetworkId,
        tx::*,
    };
    use std::str::FromStr;

    #[test]
    fn test_mass_storage() {
        // Tx with less outs than ins
        let mut tx = generate_tx_from_amounts(&[100, 200, 300], &[300, 300]);
        let test_version = Kip9Version::Alpha;

        // Assert the formula: max( 0 , C·( |O|/H(O) - |I|/A(I) ) )

        let storage_mass =
            MassCalculator::new(0, 0, 0, 10u64.pow(12)).calc_tx_storage_mass(&tx.as_verifiable(), test_version).unwrap();
        assert_eq!(storage_mass, 0); // Compounds from 3 to 2, with symmetric outputs and no fee, should be zero

        // Create asymmetry
        tx.tx.outputs[0].value = 50;
        tx.tx.outputs[1].value = 550;
        let storage_mass_parameter = 10u64.pow(12);
        let storage_mass =
            MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_tx_storage_mass(&tx.as_verifiable(), test_version).unwrap();
        assert_eq!(storage_mass, storage_mass_parameter / 50 + storage_mass_parameter / 550 - 3 * (storage_mass_parameter / 200));

        // Create a tx with more outs than ins
        let base_value = 10_000 * SOMPI_PER_KASPA;
        let mut tx = generate_tx_from_amounts(&[base_value, base_value, base_value * 2], &[base_value; 4]);
        let storage_mass_parameter = STORAGE_MASS_PARAMETER;
        let storage_mass =
            MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_tx_storage_mass(&tx.as_verifiable(), test_version).unwrap();
        assert_eq!(storage_mass, 4); // Inputs are above C so they don't contribute negative mass, 4 outputs exactly equal C each charge 1

        let mut tx2 = tx.clone();
        tx2.tx.outputs[0].value = 10 * SOMPI_PER_KASPA;
        let storage_mass =
            MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_tx_storage_mass(&tx2.as_verifiable(), test_version).unwrap();
        assert_eq!(storage_mass, 1003);

        // Increase values over the lim
        for out in tx.tx.outputs.iter_mut() {
            out.value += 1
        }
        tx.entries[0].as_mut().unwrap().amount += tx.tx.outputs.len() as u64;
        let storage_mass =
            MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_tx_storage_mass(&tx.as_verifiable(), test_version).unwrap();
        assert_eq!(storage_mass, 0);
    }

    #[test]
    fn test_mass_storage_beta() {
        // 2:2 transaction
        let mut tx = generate_tx_from_amounts(&[100, 200], &[50, 250]);
        let storage_mass_parameter = 10u64.pow(12);
        let test_version = Kip9Version::Beta;
        // Assert the formula: max( 0 , C·( |O|/H(O) - |I|/O(I) ) )

        let storage_mass =
            MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_tx_storage_mass(&tx.as_verifiable(), test_version).unwrap();
        assert_eq!(storage_mass, 9000000000);

        // Set outputs to be equal to inputs
        tx.tx.outputs[0].value = 100;
        tx.tx.outputs[1].value = 200;
        let storage_mass =
            MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_tx_storage_mass(&tx.as_verifiable(), test_version).unwrap();
        assert_eq!(storage_mass, 0);

        // Remove an output and make sure the other is small enough to make storage mass greater than zero
        tx.tx.outputs.pop();
        tx.tx.outputs[0].value = 50;
        let storage_mass =
            MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_tx_storage_mass(&tx.as_verifiable(), test_version).unwrap();
        assert_eq!(storage_mass, 5000000000);
    }

    fn generate_tx_from_amounts(ins: &[u64], outs: &[u64]) -> MutableTransaction<Transaction> {
        let script_pub_key = ScriptVec::from_slice(&[]);
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let tx = Transaction::new(
            0,
            (0..ins.len())
                .map(|i| TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: i as u32 },
                    signature_script: vec![],
                    sequence: 0,
                    sig_op_count: 0,
                })
                .collect(),
            outs.iter()
                .copied()
                .map(|out_amount| TransactionOutput {
                    value: out_amount,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
                })
                .collect(),
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );
        let entries = ins
            .iter()
            .copied()
            .map(|in_amount| UtxoEntry {
                amount: in_amount,
                script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
                block_daa_score: 0,
                is_coinbase: false,
            })
            .collect();
        MutableTransaction::with_entries(tx, entries)
    }
}
