use kaspa_consensus_core::{
    hashing,
    tx::{CovenantBinding, VerifiableTransaction},
};
use kaspa_hashes::Hash;
use kaspa_txscript_errors::CovenantsError;
use std::{collections::HashMap, sync::LazyLock};

/// Context for an input's specific authority over a subset of outputs.
///
/// Used by scripts to verify the state transitions they directly authorized
/// (e.g., 1-to-N splits) without scanning unrelated outputs.
#[cfg_attr(test, derive(PartialEq, Eq, Debug))]
pub struct CovenantInputContext {
    /// The covenant ID shared by this input and its authorized outputs.
    pub _covenant_id: Hash, // TODO(pre-covpp): Remove if unused.

    /// Indices of outputs that explicitly declare this input as their `authorizing_input`.
    ///
    /// This defines the input's direct "children" in the transaction.
    pub auth_outputs: Vec<usize>,
}

impl CovenantInputContext {
    pub fn new(covenant_id: Hash) -> Self {
        Self { _covenant_id: covenant_id, auth_outputs: Default::default() }
    }
}

/// Context for the shared transaction-wide state of a specific Covenant ID.
///
/// Used for verifying global invariants across all participants of the same covenant
/// (e.g., merges, batching, or conservation of amounts).
#[derive(Default)]
#[cfg_attr(test, derive(PartialEq, Eq, Debug))]
pub struct CovenantSharedContext {
    /// Indices of *all* inputs in the transaction carrying this `covenant_id`.
    pub input_indices: Vec<usize>,

    /// Indices of *all* outputs in the transaction carrying this `covenant_id`.
    pub output_indices: Vec<usize>,
}

/// Pre-computed cache mapping inputs and covenant ids to their execution contexts.
///
/// Enables O(1) access for covenant introspection opcodes.
#[derive(Default)]
#[cfg_attr(test, derive(PartialEq, Eq, Debug))]
pub struct CovenantsContext {
    /// Maps an input index to its local authority context.
    pub input_ctxs: HashMap<usize, CovenantInputContext>,

    /// Maps a covenant id to its shared transaction-wide context.
    pub shared_ctxs: HashMap<Hash, CovenantSharedContext>,
}

pub static EMPTY_COV_CONTEXT: LazyLock<CovenantsContext> = LazyLock::new(CovenantsContext::default);

impl CovenantsContext {
    /// Returns the absolute transaction output index of the k-th authorized output.
    ///
    /// Missing input contexts are treated as having zero authorized outputs.
    pub(crate) fn auth_output_index(&self, input_idx: usize, k: usize) -> Result<usize, CovenantsError> {
        let auth_outputs = self.input_ctxs.get(&input_idx).map(|ctx| ctx.auth_outputs.as_slice()).unwrap_or_default();
        auth_outputs.get(k).copied().ok_or(CovenantsError::InvalidAuthCovOutIndex(k, input_idx, auth_outputs.len()))
    }

    /// Returns the number of outputs authorized by this input.
    ///
    /// Missing input contexts are treated as having zero authorized outputs.
    pub(crate) fn num_auth_outputs(&self, input_idx: usize) -> usize {
        self.input_ctxs.get(&input_idx).map_or(0, |ctx| ctx.auth_outputs.len())
    }

    pub(crate) fn num_covenant_inputs(&self, covenant_id: Hash) -> usize {
        self.shared_ctxs.get(&covenant_id).map_or(0, |ctx| ctx.input_indices.len())
    }

    pub(crate) fn covenant_input_index(&self, covenant_id: Hash, k: usize) -> Result<usize, CovenantsError> {
        let input_indices = self.shared_ctxs.get(&covenant_id).map(|ctx| ctx.input_indices.as_slice()).unwrap_or_default();
        input_indices.get(k).copied().ok_or(CovenantsError::InvalidCovInIndex(covenant_id, k))
    }

    pub(crate) fn num_covenant_outputs(&self, covenant_id: Hash) -> usize {
        self.shared_ctxs.get(&covenant_id).map_or(0, |ctx| ctx.output_indices.len())
    }

    pub(crate) fn covenant_output_index(&self, covenant_id: Hash, k: usize) -> Result<usize, CovenantsError> {
        let output_indices = self.shared_ctxs.get(&covenant_id).map(|ctx| ctx.output_indices.as_slice()).unwrap_or_default();
        output_indices.get(k).copied().ok_or(CovenantsError::InvalidCovOutIndex(covenant_id, k))
    }

    /// Constructs the covenants execution context for a transaction.
    ///
    /// Collects per-input and shared covenant relations from the transaction,
    /// validating covenant bindings and handling both continuation and genesis
    /// cases. Genesis outputs are validated but do not populate covenant contexts.
    pub fn from_tx(tx: &impl VerifiableTransaction) -> Result<Self, CovenantsError> {
        let mut ctx = CovenantsContext::default();

        // Aggregated per (authorizing input, covenant id) genesis groups.
        let mut genesis_ctxs: HashMap<(usize, Hash), Vec<usize>> = HashMap::new();

        for (i, (_, entry)) in tx.populated_inputs().enumerate() {
            if let Some(covenant_id) = entry.covenant_id {
                ctx.shared_ctxs.entry(covenant_id).or_default().input_indices.push(i);
            }
        }

        for (i, output) in tx.outputs().iter().enumerate() {
            let Some(CovenantBinding { covenant_id, authorizing_input }) = output.covenant else {
                continue;
            };

            let auth_input_idx = authorizing_input as usize;

            let Some(utxo_entry) = tx.utxo(auth_input_idx) else {
                return Err(CovenantsError::AuthInputOutOfBounds(i, authorizing_input));
            };

            match utxo_entry.covenant_id {
                Some(input_covenant_id) if input_covenant_id == covenant_id => {
                    // Continuation case: the authorizing input already carries the same covenant id.
                    // Record the output under both the per-input context and the shared covenant context.

                    ctx.input_ctxs
                        .entry(auth_input_idx)
                        .or_insert_with(|| CovenantInputContext::new(covenant_id))
                        .auth_outputs
                        .push(i);

                    ctx.shared_ctxs
                        .get_mut(&covenant_id)
                        .expect("Shared context should've been created by the authorizing input")
                        .output_indices
                        .push(i);
                }
                Some(_) | None => {
                    // Genesis case: the authorizing input does not carry this covenant id (either absent or different).
                    // Treat the output as a genesis-authorized output for the pair (authorizing input, covenant id).
                    // These relations are validated via covenant-id reconstruction, but are not added to the script-engine contexts.

                    genesis_ctxs.entry((auth_input_idx, covenant_id)).or_default().push(i);
                }
            }
        }

        // Validate genesis covenant ids by recomputing the id for each (authorizing input, covenant id) group
        // from the genesis outpoint and the group's authorized outputs.
        for ((auth_input_idx, covenant_id), output_indices) in genesis_ctxs.into_iter() {
            let input = tx.inputs().get(auth_input_idx).expect("utxo(auth_input) existed above");

            let expected_id = hashing::covenant_id::covenant_id(
                input.previous_outpoint,
                output_indices.into_iter().map(|i| (i as u32, tx.outputs().get(i).expect("enumerated above"))),
            );

            if expected_id != covenant_id {
                return Err(CovenantsError::WrongGenesisCovenantId(auth_input_idx, covenant_id));
            }
        }

        Ok(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        hashing,
        subnets::SubnetworkId,
        tx::{
            CovenantBinding, PopulatedTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        },
    };

    struct OutputConfig {
        value: u64,
        authorizing_input: usize,
        covenant_group: u64, // Outputs with the same group id share a covenant id
    }

    /// Creates a transaction with configurable inputs and outputs for testing both genesis and continuation cases.
    ///
    /// - `input_covenant_ids`: Covenant id for each input (None for no covenant, Some for covenant-carrying inputs)
    /// - `outputs`: Configuration for each output
    /// - `compute_correct_ids`: If true, computes covenant ids for genesis outputs via hashing;
    ///   continuation outputs (where input covenant matches covenant_group) use covenant_group as-is
    fn create_genesis_tx(
        input_covenant_ids: Vec<Option<u64>>,
        outputs: Vec<OutputConfig>,
        compute_correct_ids: bool,
    ) -> (Transaction, Vec<UtxoEntry>) {
        // Create inputs and UTXOs
        let inputs: Vec<_> = input_covenant_ids
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let outpoint = TransactionOutpoint::new((i as u64).into(), 0);
                TransactionInput::new(outpoint, vec![], 0, 0)
            })
            .collect();

        let utxos: Vec<_> = input_covenant_ids
            .iter()
            .map(|&cov_id| UtxoEntry {
                amount: 1000,
                script_public_key: Default::default(),
                block_daa_score: 0,
                is_coinbase: false,
                covenant_id: cov_id.map(|id| id.into()),
            })
            .collect();

        // Create outputs with placeholder covenant ids
        let tx_outputs: Vec<_> = outputs
            .iter()
            .map(|cfg| TransactionOutput {
                value: cfg.value,
                script_public_key: Default::default(),
                covenant: Some(CovenantBinding {
                    covenant_id: cfg.covenant_group.into(), // Correct for continuation, placeholder for genesis
                    authorizing_input: cfg.authorizing_input as u16,
                }),
            })
            .collect();

        let mut tx = Transaction::new(0, inputs, tx_outputs, 0, SubnetworkId::default(), 0, vec![]);

        if compute_correct_ids {
            // Collect genesis outputs and compute their covenant ids (continuation outputs already have correct covenant_id == covenant_group)
            let mut genesis_groups: HashMap<(usize, u64), Vec<usize>> = HashMap::new();

            for (i, cfg) in outputs.iter().enumerate() {
                let input_cov_id = input_covenant_ids.get(cfg.authorizing_input).copied().flatten();

                if input_cov_id != Some(cfg.covenant_group) {
                    genesis_groups.entry((cfg.authorizing_input, cfg.covenant_group)).or_default().push(i);
                }
            }

            for ((auth_input_idx, _), output_indices) in genesis_groups {
                let outpoint = TransactionOutpoint::new((auth_input_idx as u64).into(), 0);
                let expected_id =
                    hashing::covenant_id::covenant_id(outpoint, output_indices.iter().map(|&i| (i as u32, &tx.outputs[i])));

                for &output_idx in &output_indices {
                    tx.outputs[output_idx].covenant.as_mut().unwrap().covenant_id = expected_id;
                }
            }
        }

        tx.finalize();
        (tx, utxos)
    }

    #[test]
    fn test_genesis_single_output() {
        let (tx, entries) =
            create_genesis_tx(vec![None], vec![OutputConfig { value: 100, authorizing_input: 0, covenant_group: 1 }], true);
        let populated_tx = PopulatedTransaction::new(&tx, entries);
        let ctx = CovenantsContext::from_tx(&populated_tx).unwrap();

        // For genesis, contexts should be empty since genesis outputs don't populate contexts
        assert!(ctx.input_ctxs.is_empty());
        assert!(ctx.shared_ctxs.is_empty());
    }

    #[test]
    fn test_genesis_multiple_outputs() {
        let (tx, entries) = create_genesis_tx(
            vec![None],
            vec![
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 1 },
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 1 },
            ],
            true,
        );
        let populated_tx = PopulatedTransaction::new(&tx, entries);
        let ctx = CovenantsContext::from_tx(&populated_tx).unwrap();

        // Genesis contexts empty
        assert!(ctx.input_ctxs.is_empty());
        assert!(ctx.shared_ctxs.is_empty());
    }

    #[test]
    fn test_genesis_invalid_covenant_id() {
        let (tx, entries) = create_genesis_tx(
            vec![None],
            vec![OutputConfig { value: 100, authorizing_input: 0, covenant_group: 1 }],
            false, // Use wrong covenant ids
        );
        let populated_tx = PopulatedTransaction::new(&tx, entries);
        let result = CovenantsContext::from_tx(&populated_tx);
        assert!(matches!(result, Err(CovenantsError::WrongGenesisCovenantId(0, _))));
    }

    #[test]
    fn test_genesis_single_input_multiple_covenant_groups() {
        // Three outputs with two different covenant groups from the same input
        let (tx, entries) = create_genesis_tx(
            vec![None],
            vec![
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 1 }, // Group A
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 2 }, // Group B
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 1 }, // Group A
            ],
            true,
        );
        let populated_tx = PopulatedTransaction::new(&tx, entries);
        let ctx = CovenantsContext::from_tx(&populated_tx).unwrap();

        // Genesis contexts empty
        assert!(ctx.input_ctxs.is_empty());
        assert!(ctx.shared_ctxs.is_empty());
    }

    #[test]
    fn test_genesis_multiple_inputs() {
        let (tx, entries) = create_genesis_tx(
            vec![None, None],
            vec![
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 1 },
                OutputConfig { value: 100, authorizing_input: 1, covenant_group: 2 },
            ],
            true,
        );
        let populated_tx = PopulatedTransaction::new(&tx, entries);
        let ctx = CovenantsContext::from_tx(&populated_tx).unwrap();

        // Genesis contexts empty
        assert!(ctx.input_ctxs.is_empty());
        assert!(ctx.shared_ctxs.is_empty());
    }

    #[test]
    fn test_continuation_with_genesis() {
        // Complex case: 1 input with covenant id 42, creating:
        // - 1 continuation output (covenant 42)
        // - 2 genesis outputs (group A, non-contiguous at indices 1 and 3)
        // - 1 genesis output (group B, at index 2)
        // This tests a single input both continuing its covenant and creating new genesis covenants
        let (tx, entries) = create_genesis_tx(
            vec![Some(42)],
            vec![
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 42 }, // Continuation
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 100 }, // Genesis A
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 200 }, // Genesis B
                OutputConfig { value: 100, authorizing_input: 0, covenant_group: 100 }, // Genesis A
            ],
            true,
        );
        let populated_tx = PopulatedTransaction::new(&tx, entries);
        let actual_ctx = CovenantsContext::from_tx(&populated_tx).unwrap();

        // Build expected context: input 0 continues covenant 42 with output 0
        // Genesis outputs (1, 2, 3) should not appear in contexts
        let covenant_42: Hash = 42u64.into();
        let expected_ctx = CovenantsContext {
            input_ctxs: HashMap::from_iter([(0, CovenantInputContext { _covenant_id: covenant_42, auth_outputs: vec![0] })]),
            shared_ctxs: HashMap::from_iter([(
                covenant_42,
                CovenantSharedContext { input_indices: vec![0], output_indices: vec![0] },
            )]),
        };

        assert_eq!(actual_ctx, expected_ctx);
    }

    #[test]
    fn test_authorizing_input_out_of_bounds() {
        // Create a transaction with an output referencing a non-existent input
        let input = TransactionInput::new(TransactionOutpoint::new(1u64.into(), 0), vec![], 0, 0);
        let utxo = UtxoEntry {
            amount: 1000,
            script_public_key: Default::default(),
            block_daa_score: 0,
            is_coinbase: false,
            covenant_id: None,
        };

        let output = TransactionOutput {
            value: 100,
            script_public_key: Default::default(),
            covenant: Some(CovenantBinding {
                covenant_id: 1u64.into(),
                authorizing_input: 1, // Out of bounds - only 1 input (index 0)
            }),
        };

        let mut tx = Transaction::new(0, vec![input], vec![output], 0, SubnetworkId::default(), 0, vec![]);
        tx.finalize();

        let populated_tx = PopulatedTransaction::new(&tx, vec![utxo]);
        let result = CovenantsContext::from_tx(&populated_tx);
        assert!(matches!(result, Err(CovenantsError::AuthInputOutOfBounds(0, 1))));
    }

    #[test]
    fn test_no_covenant_outputs() {
        // Create a transaction with outputs that have no covenants
        let input = TransactionInput::new(TransactionOutpoint::new(1u64.into(), 0), vec![], 0, 0);
        let utxo = UtxoEntry {
            amount: 1000,
            script_public_key: Default::default(),
            block_daa_score: 0,
            is_coinbase: false,
            covenant_id: Some(42u64.into()),
        };

        // Outputs with no covenant bindings
        let output1 = TransactionOutput::new(100, Default::default());
        let output2 = TransactionOutput::new(200, Default::default());

        let mut tx = Transaction::new(0, vec![input], vec![output1, output2], 0, SubnetworkId::default(), 0, vec![]);
        tx.finalize();

        let populated_tx = PopulatedTransaction::new(&tx, vec![utxo]);
        let actual_ctx = CovenantsContext::from_tx(&populated_tx).unwrap();

        // Input 0 has covenant_id, so shared context contains it
        // No outputs have covenants, so input_ctxs is empty and shared context has no output indices
        let covenant_42: Hash = 42u64.into();
        let expected_ctx = CovenantsContext {
            input_ctxs: HashMap::new(),
            shared_ctxs: HashMap::from_iter([(covenant_42, CovenantSharedContext { input_indices: vec![0], output_indices: vec![] })]),
        };

        assert_eq!(actual_ctx, expected_ctx);
    }
}
