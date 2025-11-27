use crate::{
    config::params::Params,
    constants::TRANSIENT_BYTE_TO_MASS_FACTOR,
    subnets::SUBNETWORK_ID_SIZE,
    tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutput, UtxoEntry, VerifiableTransaction},
};
use kaspa_hashes::HASH_SIZE;

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

/// Returns the UTXO storage "plurality" for this script public key.
/// i.e., how many 100-byte "storage units" it occupies.
/// The choice of 100 bytes per unit ensures that all standard SPKs have a plurality of 1.
pub fn utxo_plurality(spk: &ScriptPublicKey) -> u64 {
    /// A constant representing the number of bytes used by the fixed parts of a UTXO.
    const UTXO_CONST_STORAGE: usize =
        32  // outpoint::tx_id
        + 4 // outpoint::index
        + 8 // entry amount
        + 8 // entry DAA score
        + 1 // entry is coinbase
        + 2 // entry spk version
        + 8 // entry spk len
    ;

    // The base (63 bytes) plus the max standard public key length (33 bytes) fits into one 100-byte unit.
    // Hence, all standard SPKs end up with a plurality of 1.
    const UTXO_UNIT_SIZE: usize = 100;

    (UTXO_CONST_STORAGE + spk.script().len()).div_ceil(UTXO_UNIT_SIZE) as u64
}

pub trait UtxoPlurality {
    /// Returns the UTXO storage plurality for the script public key associated with this object.
    fn plurality(&self) -> u64;
}

impl UtxoPlurality for ScriptPublicKey {
    fn plurality(&self) -> u64 {
        utxo_plurality(self)
    }
}

impl UtxoPlurality for UtxoEntry {
    fn plurality(&self) -> u64 {
        utxo_plurality(&self.script_public_key)
    }
}

impl UtxoPlurality for TransactionOutput {
    fn plurality(&self) -> u64 {
        utxo_plurality(&self.script_public_key)
    }
}

/// An abstract UTXO storage cell.
///
/// # Plurality
///
/// Each `UtxoCell` now has a `plurality` field reflecting how many 100-byte "storage units"
/// this UTXO effectively occupies. This generalizes KIP-0009 to support UTXOs with
/// script public keys larger than the standard 33-byte limit. For a UTXO of byte-size
/// `entry.size`, we define:
///
/// ```ignore
/// p := ceil(entry.size / UTXO_UNIT)
/// ```
///
/// Conceptually, we treat a large UTXO as `p` sub-entries each holding `entry.amount / p`,
/// preserving the total locked amount but increasing the "count" proportionally to script size.
///
/// Refer to the KIP-0009 specification for more details.
#[derive(Clone, Copy)]
pub struct UtxoCell {
    /// The plurality (number of "storage units") for this UTXO
    pub plurality: u64,
    /// The amount of KAS (in sompis) locked in this UTXO
    pub amount: u64,
}

impl UtxoCell {
    pub fn new(plurality: u64, amount: u64) -> Self {
        Self { plurality, amount }
    }
}

impl From<&UtxoEntry> for UtxoCell {
    fn from(entry: &UtxoEntry) -> Self {
        Self::new(entry.plurality(), entry.amount)
    }
}

impl From<&TransactionOutput> for UtxoCell {
    fn from(output: &TransactionOutput) -> Self {
        Self::new(output.plurality(), output.value)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct NonContextualMasses {
    /// Compute mass
    pub compute_mass: u64,

    /// Transient storage mass
    pub transient_mass: u64,
}

impl NonContextualMasses {
    pub fn new(compute_mass: u64, transient_mass: u64) -> Self {
        Self { compute_mass, transient_mass }
    }

    /// Returns the maximum over all non-contextual masses (currently compute and transient). This
    /// max value has no consensus meaning and should only be used for mempool-level simplification
    /// such as obtaining a one-dimensional mass value when composing blocks templates.  
    pub fn max(&self) -> u64 {
        self.compute_mass.max(self.transient_mass)
    }
}

impl std::fmt::Display for NonContextualMasses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "compute: {}, transient: {}", self.compute_mass, self.transient_mass)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ContextualMasses {
    /// Persistent storage mass
    pub storage_mass: u64,
}

impl ContextualMasses {
    pub fn new(storage_mass: u64) -> Self {
        Self { storage_mass }
    }

    /// Returns the maximum over *all masses* (currently compute, transient and storage). This max
    /// value has no consensus meaning and should only be used for mempool-level simplification such
    /// as obtaining a one-dimensional mass value when composing blocks templates.  
    pub fn max(&self, non_contextual_masses: NonContextualMasses) -> u64 {
        self.storage_mass.max(non_contextual_masses.max())
    }
}

impl std::fmt::Display for ContextualMasses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "storage: {}", self.storage_mass)
    }
}

impl std::cmp::PartialEq<u64> for ContextualMasses {
    fn eq(&self, other: &u64) -> bool {
        self.storage_mass.eq(other)
    }
}

pub type Mass = (NonContextualMasses, ContextualMasses);

pub trait MassOps {
    fn max(&self) -> u64;
}

impl MassOps for Mass {
    fn max(&self) -> u64 {
        self.1.max(self.0)
    }
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

    /// Calculates the non-contextual masses for this transaction (i.e., masses which can be calculated from
    /// the transaction alone). These include compute and transient storage masses of this transaction. This
    /// does not include the persistent storage mass calculation below which requires full UTXO context
    pub fn calc_non_contextual_masses(&self, tx: &Transaction) -> NonContextualMasses {
        if tx.is_coinbase() {
            return NonContextualMasses::new(0, 0);
        }

        let size = transaction_estimated_serialized_size(tx);
        let compute_mass_for_size = size * self.mass_per_tx_byte;
        let total_script_public_key_size: u64 = tx
            .outputs
            .iter()
            .map(|output| 2 /* script public key version (u16) */ + output.script_public_key.script().len() as u64)
            .sum();
        let total_script_public_key_mass = total_script_public_key_size * self.mass_per_script_pub_key_byte;

        let total_sigops: u64 = tx.inputs.iter().map(|input| input.sig_op_count as u64).sum();
        let total_sigops_mass = total_sigops * self.mass_per_sig_op;

        let compute_mass = compute_mass_for_size + total_script_public_key_mass + total_sigops_mass;
        let transient_mass = size * TRANSIENT_BYTE_TO_MASS_FACTOR;

        NonContextualMasses::new(compute_mass, transient_mass)
    }

    /// Calculates the contextual masses for this populated transaction.
    /// Assumptions which must be verified before this call:
    ///     1. All output values are non-zero
    ///     2. At least one input (unless coinbase)
    ///
    /// Otherwise this function should never fail.
    pub fn calc_contextual_masses(&self, tx: &impl VerifiableTransaction) -> Option<ContextualMasses> {
        calc_storage_mass(
            tx.is_coinbase(),
            tx.populated_inputs().map(|(_, entry)| entry.into()),
            tx.outputs().iter().map(|out| out.into()),
            self.storage_mass_parameter,
        )
        .map(ContextualMasses::new)
    }
}

/// Calculates the storage mass (KIP-0009) for a given set of inputs and outputs.
///
/// This function has been generalized for UTXO entries that may exceed
/// the max standard 33-byte script public key size. Each `UtxoCell::plurality` indicates
/// how many 100-byte "storage units" that UTXO occupies.
///
/// # Formula Overview
///
/// The core formula is:
///
/// ```ignore
///     max(0, C · (|O| / H(O) - |I| / A(I)))
/// ```
///
/// where:
///
/// - `C` is the storage mass parameter (`storm_param`).
/// - `|O|` and `|I|` are the total pluralities of outputs and inputs, respectively.
/// - `H(O)` is the harmonic mean of the outputs' amounts, generalized to account for per-UTXO
///   `plurality`.
///
///   In standard KIP-0009, one has:
///
///   ```ignore
///   |O| / H(O) = Σ (1 / o)
///   ```
///
///   Here, each UTXO that occupies `p` storage units is treated as `p` sub-entries,
///   each holding `amount / p`. This effectively converts `1 / o` into `p^2 / amount`.
///   Consequently, the code accumulates:
///
///   ```ignore
///   Σ [C · p(o)^2 / amount(o)]
///   ```
///
/// - `A(I)` is the arithmetic mean of the inputs' amounts, similarly scaled by `|I|`,
///   while the sum of amounts remains unchanged.
///
/// Under the “relaxed formula” conditions (`|O| = 1`, `|I| = 1`, or `|O| = |I| = 2`),
/// we compute the harmonic mean for inputs as well; otherwise, we use the arithmetic
/// approach for inputs.
///
/// Refer to KIP-0009 for more details.
///
/// Assumptions which must be verified before this call:
///   1. All input/output values are non-zero
///   2. At least one input (unless coinbase)
///
/// If these assumptions hold, this function should never fail. A `None` return
/// indicates that the mass is incomputable and can be considered too high.
pub fn calc_storage_mass(
    is_coinbase: bool,
    inputs: impl ExactSizeIterator<Item = UtxoCell> + Clone,
    mut outputs: impl Iterator<Item = UtxoCell>,
    storm_param: u64,
) -> Option<u64> {
    if is_coinbase {
        return Some(0);
    }

    /*
        In KIP-0009 terms, the canonical formula is:
            max(0, C * (|O|/H(O) - |I|/A(I))).

        We first calculate the harmonic portion for outputs in a single pass,
        accumulating:
            1) outs_plurality = Σ p(o)
            2) harmonic_outs  = Σ [C * p(o)^2 / amount(o)]
    */
    let (outs_plurality, harmonic_outs) = outputs.try_fold(
        (0u64, 0u64), // (accumulated plurality, accumulated harmonic)
        |(acc_plurality, acc_harm), UtxoCell { plurality, amount }| {
            Some((
                acc_plurality + plurality, // represents in-memory bytes, cannot overflow
                acc_harm.checked_add(storm_param.checked_mul(plurality)?.checked_mul(plurality)? / amount)?,
            ))
        },
    )?;

    /*
        KIP-0009 defines a relaxed formula for the cases:
            |O| = 1  or  |O| <= |I| <= 2

        The relaxed formula is:
            max(0, C · (|O| / H(O) - |I| / H(I)))

        If |I| = 1, the harmonic and arithmetic approaches coincide, so the conditions can be expressed as:
            |O| = 1 or |I| = 1 or |O| = |I| = 2
    */
    let relaxed_formula_path = {
        if outs_plurality == 1 {
            true // |O| = 1
        } else if inputs.len() > 2 {
            false // since element plurality always >= 1 => ins_plurality > 2 => skip harmonic path
        } else {
            // For <= 2 inputs, we can afford to clone and sum the pluralities
            let ins_plurality = inputs.clone().map(|cell| cell.plurality).sum::<u64>();
            ins_plurality == 1 || (outs_plurality == 2 && ins_plurality == 2)
        }
    };

    if relaxed_formula_path {
        // Each input i contributes C · p(i)^2 / amount(i)
        let harmonic_ins = inputs
            .map(|UtxoCell { plurality, amount }| storm_param * plurality * plurality / amount) // we assume no overflow (see verify_utxo_plurality_limits)
            .fold(0u64, |total, current| total.saturating_add(current));

        // max(0, harmonic_outs - harmonic_ins)
        return Some(harmonic_outs.saturating_sub(harmonic_ins));
    }

    // Otherwise, we calculate the arithmetic portion for inputs:
    // (ins_plurality, sum_ins) =>  (Σ plurality, Σ amounts)
    let (ins_plurality, sum_ins) =
        inputs.fold((0u64, 0u64), |(acc_plur, acc_amt), UtxoCell { plurality, amount }| (acc_plur + plurality, acc_amt + amount));

    // mean_ins = (Σ amounts) / (Σ plurality)
    let mean_ins = sum_ins / ins_plurality;

    // arithmetic_ins:  C · (|I| / A(I)) = |I| · (C / mean_ins)
    let arithmetic_ins = ins_plurality.saturating_mul(storm_param / mean_ins);

    // max(0, harmonic_outs - arithmetic_ins)
    Some(harmonic_outs.saturating_sub(arithmetic_ins))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        constants::{SOMPI_PER_KASPA, STORAGE_MASS_PARAMETER},
        network::NetworkType,
        subnets::SubnetworkId,
        tx::*,
    };
    use std::str::FromStr;

    const UTXO_CONST_STORAGE: u64 = 63;
    const UTXO_UNIT_SIZE: u64 = 100;

    #[test]
    fn verify_utxo_plurality_limits() {
        /*
           Verify that for all networks, existing UTXO entries can never overflow the product C·P^2 used
           for harmonic_ins within calc_storage_mass
        */
        for net in NetworkType::iter() {
            let params: Params = net.into();
            let max_spk_len = (params.max_script_public_key_len().after() as u64)
                .min(params.max_block_mass.div_ceil(params.mass_per_script_pub_key_byte));
            let max_plurality = (UTXO_CONST_STORAGE + max_spk_len).div_ceil(UTXO_UNIT_SIZE); // see utxo_plurality
            let product = params.storage_mass_parameter.checked_mul(max_plurality).and_then(|x| x.checked_mul(max_plurality));
            // verify C·P^2 can never overflow
            assert!(product.is_some());
        }

        // verify P >= 1 also when the script is empty
        assert!(utxo_plurality(&ScriptPublicKey::new(0, ScriptVec::from_slice(&[]))) == 1);
        // Assert the UTXO_CONST_STORAGE=63, UTXO_UNIT_SIZE=100 constants
        assert!(utxo_plurality(&ScriptPublicKey::from_vec(0, vec![1; (UTXO_UNIT_SIZE - UTXO_CONST_STORAGE) as usize])) == 1);
        assert!(utxo_plurality(&ScriptPublicKey::from_vec(0, vec![1; (UTXO_UNIT_SIZE - UTXO_CONST_STORAGE + 1) as usize])) == 2);
    }

    #[derive(Debug)]
    struct PluralityTestCase {
        /// Test name
        name: &'static str,

        /// Amounts for the first transaction's inputs
        inputs_tx1: &'static [u64],
        /// Amounts for the first transaction's outputs
        outputs_tx1: &'static [u64],

        /// Amounts for the second transaction's inputs
        inputs_tx2: &'static [u64],
        /// Amounts for the second transaction's outputs
        outputs_tx2: &'static [u64],

        /// (Optional) index of the input/output in tx2 whose script we want to override
        plurality_index: Option<usize>,
        /// Desired plurality for that UTXO's script
        desired_plurality: Option<u64>,
        /// Whether to override an output and not an input
        override_output: bool,

        /// Mass calculator parameters
        storage_mass_parameter: u64,
    }

    impl PluralityTestCase {
        /// Runs the test and asserts that the masses are equal
        fn run(&self) {
            // Sanity
            assert!(
                self.inputs_tx1.iter().sum::<u64>() >= self.outputs_tx1.iter().sum::<u64>(),
                "Test \"{}\": tx1 outs > ins",
                self.name
            );
            assert!(
                self.inputs_tx2.iter().sum::<u64>() >= self.outputs_tx2.iter().sum::<u64>(),
                "Test \"{}\": tx2 outs > ins",
                self.name
            );

            // Generate
            let tx1 = generate_tx_from_amounts(self.inputs_tx1, self.outputs_tx1);
            let mut tx2 = generate_tx_from_amounts(self.inputs_tx2, self.outputs_tx2);

            // If specified, override one of the script public keys in tx2.
            if let (Some(index), Some(plur)) = (self.plurality_index, self.desired_plurality) {
                if self.override_output {
                    tx2.tx.outputs[index].script_public_key = generate_script_for_plurality(plur);
                } else {
                    tx2.entries[index].as_mut().unwrap().script_public_key = generate_script_for_plurality(plur);
                }
            }

            let mc = MassCalculator::new(0, 0, 0, self.storage_mass_parameter);

            let mass1 = mc.calc_contextual_masses(&tx1.as_verifiable());
            let mass2 = mc.calc_contextual_masses(&tx2.as_verifiable());

            assert_ne!(mass1, Some(ContextualMasses::new(0)), "Test \"{}\": avoid running meaningless test cases", self.name);
            assert_eq!(mass1, mass2, "Test \"{}\" failed: mass1 = {:?}, mass2 = {:?}", self.name, mass1, mass2);
        }
    }

    #[test]
    fn test_storage_mass_pluralities() {
        /*
            Tests pluralities by comparing transactions with all inputs/outputs with plurality 1
            with transactions with a super entry (plurality > 1) which replaces several entries
            in the plurality 1 tx (with equal total value and equal total plurality)
        */
        let test_cases = vec![
            PluralityTestCase {
                name: "3:4; input index=1, plurality=2",
                inputs_tx1: &[300, 200, 200],
                outputs_tx1: &[200, 200, 200, 100],
                inputs_tx2: &[300, 400],
                outputs_tx2: &[200, 200, 200, 100],
                plurality_index: Some(1),
                desired_plurality: Some(2),
                override_output: false,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "2:3; output index=1, plurality=2",
                inputs_tx1: &[350, 400],
                outputs_tx1: &[300, 200, 200],
                inputs_tx2: &[350, 400],
                outputs_tx2: &[300, 400],
                plurality_index: Some(1),
                desired_plurality: Some(2),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "1:2; output index=0, plurality=2",
                inputs_tx1: &[500],
                outputs_tx1: &[200, 200],
                inputs_tx2: &[500],
                outputs_tx2: &[400],
                plurality_index: Some(0),
                desired_plurality: Some(2),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "1:3; output index=1, plurality=2",
                inputs_tx1: &[1000],
                outputs_tx1: &[200, 200, 200],
                inputs_tx2: &[1000],
                outputs_tx2: &[200, 400],
                plurality_index: Some(1),
                desired_plurality: Some(2),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "1:3; output index=1, plurality=2; kas units",
                inputs_tx1: &[1000 * SOMPI_PER_KASPA],
                outputs_tx1: &[200 * SOMPI_PER_KASPA, 200 * SOMPI_PER_KASPA, 200 * SOMPI_PER_KASPA],
                inputs_tx2: &[1000 * SOMPI_PER_KASPA],
                outputs_tx2: &[200 * SOMPI_PER_KASPA, 400 * SOMPI_PER_KASPA],
                plurality_index: Some(1),
                desired_plurality: Some(2),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "1:2; output index=0, plurality=2; kas units",
                inputs_tx1: &[1000 * SOMPI_PER_KASPA],
                outputs_tx1: &[200 * SOMPI_PER_KASPA, 200 * SOMPI_PER_KASPA],
                inputs_tx2: &[1000 * SOMPI_PER_KASPA],
                outputs_tx2: &[400 * SOMPI_PER_KASPA],
                plurality_index: Some(0),
                desired_plurality: Some(2),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "2:2; output index=0, plurality=2; kas units",
                inputs_tx1: &[350 * SOMPI_PER_KASPA, 500 * SOMPI_PER_KASPA],
                outputs_tx1: &[200 * SOMPI_PER_KASPA, 200 * SOMPI_PER_KASPA],
                inputs_tx2: &[350 * SOMPI_PER_KASPA, 500 * SOMPI_PER_KASPA],
                outputs_tx2: &[400 * SOMPI_PER_KASPA],
                plurality_index: Some(0),
                desired_plurality: Some(2),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "4:6; output index=0, plurality=3; kas units",
                inputs_tx1: &[350 * SOMPI_PER_KASPA, 500 * SOMPI_PER_KASPA, 350 * SOMPI_PER_KASPA, 500 * SOMPI_PER_KASPA],
                outputs_tx1: &[
                    200 * SOMPI_PER_KASPA,
                    200 * SOMPI_PER_KASPA,
                    400 * SOMPI_PER_KASPA,
                    250 * SOMPI_PER_KASPA,
                    250 * SOMPI_PER_KASPA,
                    250 * SOMPI_PER_KASPA,
                ],
                inputs_tx2: &[350 * SOMPI_PER_KASPA, 500 * SOMPI_PER_KASPA, 350 * SOMPI_PER_KASPA, 500 * SOMPI_PER_KASPA],
                outputs_tx2: &[200 * SOMPI_PER_KASPA, 200 * SOMPI_PER_KASPA, 400 * SOMPI_PER_KASPA, 750 * SOMPI_PER_KASPA],
                plurality_index: Some(3),
                desired_plurality: Some(3),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
        ];

        for tc in test_cases {
            tc.run();
        }
    }

    /// ScriptPublicKey generator that yields a script with length adjusted
    /// to match `desired_plurality`.
    fn generate_script_for_plurality(desired_plurality: u64) -> ScriptPublicKey {
        let required_script_len = ((desired_plurality - 1) * UTXO_UNIT_SIZE) as usize;
        ScriptPublicKey::from_vec(0, vec![1; required_script_len])
    }

    #[test]
    fn test_storage_mass() {
        // Tx with less outs than ins
        let mut tx = generate_tx_from_amounts(&[100, 200, 300], &[300, 300]);

        //
        // Assert the formula: max( 0 , C·( |O|/H(O) - |I|/A(I) ) )
        //

        let storage_mass = MassCalculator::new(0, 0, 0, 10u64.pow(12)).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 0); // Compounds from 3 to 2, with symmetric outputs and no fee, should be zero

        // Create asymmetry
        tx.tx.outputs[0].value = 50;
        tx.tx.outputs[1].value = 550;
        let storage_mass_parameter = 10u64.pow(12);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, storage_mass_parameter / 50 + storage_mass_parameter / 550 - 3 * (storage_mass_parameter / 200));

        // Create a tx with more outs than ins
        let base_value = 10_000 * SOMPI_PER_KASPA;
        let mut tx = generate_tx_from_amounts(&[base_value, base_value, base_value * 2], &[base_value; 4]);
        let storage_mass_parameter = STORAGE_MASS_PARAMETER;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 4); // Inputs are above C so they don't contribute negative mass, 4 outputs exactly equal C each charge 1

        let mut tx2 = tx.clone();
        tx2.tx.outputs[0].value = 10 * SOMPI_PER_KASPA;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx2.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 1003);

        // Increase values over the lim
        for out in tx.tx.outputs.iter_mut() {
            out.value += 1
        }
        tx.entries[0].as_mut().unwrap().amount += tx.tx.outputs.len() as u64;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 0);

        // Now create 2:2 transaction
        // Assert the formula: max( 0 , C·( |O|/H(O) - |I|/H(I) ) )
        let mut tx = generate_tx_from_amounts(&[100, 200], &[50, 250]);
        let storage_mass_parameter = 10u64.pow(12);

        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 9000000000);

        // Set outputs to be equal to inputs
        tx.tx.outputs[0].value = 100;
        tx.tx.outputs[1].value = 200;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 0);

        // Remove an output and make sure the other is small enough to make storage mass greater than zero
        tx.tx.outputs.pop();
        tx.tx.outputs[0].value = 50;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
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
