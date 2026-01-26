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
                return Err(CovenantsError::WrongGenesisCovenantId(auth_input_idx));
            }
        }

        Ok(ctx)
    }
}
