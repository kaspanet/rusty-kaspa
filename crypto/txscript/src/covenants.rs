use kaspa_consensus_core::tx::{CovenantBinding, VerifiableTransaction};
use kaspa_hashes::Hash;
use kaspa_txscript_errors::{CovenantsError, TxScriptError};
use std::{collections::HashMap, sync::LazyLock};

/// Context for an input's specific authority over a subset of outputs.
///
/// Used by scripts to verify the state transitions they directly authorized
/// (e.g., 1-to-N splits) without scanning unrelated outputs.
pub struct CovenantInputContext {
    /// The covenant ID shared by this input and its authorized outputs.
    pub covenant_id: Hash,

    /// Indices of outputs that explicitly declare this input as their `authorizing_input`.
    ///
    /// This defines the input's direct "children" in the transaction.
    pub auth_outputs: Vec<usize>,
}

impl CovenantInputContext {
    pub fn new(covenant_id: Hash) -> Self {
        Self { covenant_id, auth_outputs: Default::default() }
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
    pub(crate) fn auth_output_index(&self, input_idx: usize, k: usize) -> Result<usize, TxScriptError> {
        let auth_outputs = self.input_ctxs.get(&input_idx).map(|ctx| ctx.auth_outputs.as_slice()).unwrap_or(&[]);
        auth_outputs.get(k).copied().ok_or(TxScriptError::InvalidInputCovOutIndex(k, input_idx, auth_outputs.len()))
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

    pub(crate) fn covenant_input_index(&self, covenant_id: Hash, k: usize) -> Result<usize, TxScriptError> {
        let input_indices = self.shared_ctxs.get(&covenant_id).map(|ctx| ctx.input_indices.as_slice()).unwrap_or_default();
        input_indices.get(k).copied().ok_or(CovenantsError::InvalidCovInIndex(covenant_id, k).into())
    }

    pub(crate) fn num_covenant_outputs(&self, covenant_id: Hash) -> usize {
        self.shared_ctxs.get(&covenant_id).map_or(0, |ctx| ctx.output_indices.len())
    }

    pub(crate) fn covenant_output_index(&self, covenant_id: Hash, k: usize) -> Result<usize, TxScriptError> {
        let output_indices = self.shared_ctxs.get(&covenant_id).map(|ctx| ctx.output_indices.as_slice()).unwrap_or_default();
        output_indices.get(k).copied().ok_or(CovenantsError::InvalidCovOutIndex(covenant_id, k).into())
    }

    /// Constructs the covenants execution context for a transaction.
    ///
    /// Collects per-input and shared covenant relations from the transaction,
    /// validating covenant bindings and handling both continuation and genesis
    /// cases. Genesis outputs are validated but do not populate covenant contexts.
    pub fn from_tx(tx: &impl VerifiableTransaction) -> Result<Self, CovenantsError> {
        let mut ctx = CovenantsContext::default();

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
                    // Non-genesis case: the authorizing input is already under a covenant.
                    // Add the output to the input- and covenant-level contexts.

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
                Some(_) => {
                    return Err(CovenantsError::WrongCovenantId(i));
                }
                None => {
                    // Genesis case: the authorizing input is not under a covenant yet.
                    // No covenant script is expected to run at this point, so we only validate.

                    let input = &tx.inputs()[auth_input_idx]; // safe: utxo(auth_input) existed above
                    let expected_id = kaspa_consensus_core::hashing::covenant_id::covenant_id(input.previous_outpoint);

                    if expected_id != covenant_id {
                        return Err(CovenantsError::WrongGenesisCovenantId(i));
                    }
                }
            }
        }

        Ok(ctx)
    }
}
