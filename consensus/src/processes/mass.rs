use kaspa_consensus_core::{
    mass::transaction_estimated_serialized_size,
    tx::{Transaction, VerifiableTransaction},
};

// TODO (aspect) - review and potentially merge this with the new MassCalculator currently located in the wallet core
// (i.e. migrate mass calculator from wallet core here or to consensus core)
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

    pub fn calc_tx_mass(&self, tx: &Transaction) -> u64 {
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
    pub fn calc_tx_storage_mass(&self, tx: &impl VerifiableTransaction) -> Option<u64> {
        if tx.is_coinbase() {
            return Some(0);
        }
        /* The code below computes the following formula:

                max( 0 , C·( |O|/H(O) - |I|/A(I) ) )

        where C is the mass storage parameter, O is the set of output values, I is the set of
        input values, H(S) := |S|/sum_{s in S} 1 / s is the harmonic mean over the set S and
        A(S) := sum_{s in S} / |S| is the arithmetic mean.

        See the (to date unpublished) KIP-0009 for more details
        */

        // Since we are doing integer division, we perform the multiplication with C over the inner
        // fractions, otherwise we'll get a sum of zeros or ones.
        //
        // If sum of fractions overflowed (nearly impossible, requires 10^7 outputs for C = 10^12),
        // we return `None` indicating mass is incomputable
        let harmonic_outs = tx
            .tx()
            .outputs
            .iter()
            .map(|out| self.storage_mass_parameter / out.value)
            .try_fold(0u64, |total, current| total.checked_add(current))?; // C·|O|/H(O)

        // Total supply is bounded, so a sum of existing UTXO entries cannot overflow (nor can it be zero)
        let sum_ins = tx.populated_inputs().map(|(_, entry)| entry.amount).sum::<u64>(); // |I|·A(I)
        let ins_len = tx.tx().inputs.len() as u64;
        let mean_ins = sum_ins / ins_len;

        // Inner fraction must be with C and over the mean value, in order to maximize precision.
        // We can saturate the overall expression at u64::MAX since we lower-bound the subtraction below by zero anyway
        let arithmetic_ins = ins_len.saturating_mul(self.storage_mass_parameter / mean_ins); // C·|I|/A(I)

        Some(harmonic_outs.saturating_sub(arithmetic_ins)) // max( 0 , C·( |O|/H(O) - |I|/A(I) ) )
    }
}

// TODO: tests
