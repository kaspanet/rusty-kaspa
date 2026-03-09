use std::rc::Rc;

use risc0_zkvm::{ExecutorEnv, ExternalProver, LocalProver, Prover, ProverOpts, Receipt};
use zk_covenant_rollup_core::PublicInput;
use zk_covenant_rollup_methods::ZK_COVENANT_ROLLUP_GUEST_ELF;

use crate::mock_tx::ZkTransaction;

/// Which risc0 prover backend to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProverBackend {
    /// Local in-process prover. Uses CPU normally, GPU when built with `cuda` feature.
    Local,
    /// External prover via `r0vm` subprocess (IPC over Unix socket).
    Ipc,
}

impl ProverBackend {
    pub fn label(&self) -> &'static str {
        match self {
            ProverBackend::Local => {
                if cfg!(feature = "cuda") {
                    "Local (GPU)"
                } else {
                    "Local (CPU)"
                }
            }
            ProverBackend::Ipc => "IPC (r0vm)",
        }
    }

    pub fn next(self) -> Self {
        match self {
            ProverBackend::Local => ProverBackend::Ipc,
            ProverBackend::Ipc => ProverBackend::Local,
        }
    }
}

/// Which type of proof to generate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofKind {
    /// STARK-based succinct proof (constant size).
    Succinct,
    /// Groth16 SNARK proof (smallest, requires Docker for proving).
    Groth16,
}

impl ProofKind {
    pub fn label(&self) -> &'static str {
        match self {
            ProofKind::Succinct => "Succinct (STARK)",
            ProofKind::Groth16 => "Groth16 (SNARK)",
        }
    }

    pub fn all() -> &'static [ProofKind] {
        &[ProofKind::Succinct, ProofKind::Groth16]
    }
}

/// All data needed to produce a ZK proof for a batch of rollup blocks.
pub struct ProveInput {
    /// Public input committed to the proof (prev state, prev seq, covenant ID).
    pub public_input: PublicInput,
    /// All block transactions in the proving window, grouped by block.
    pub block_txs: Vec<Vec<ZkTransaction>>,
    /// Converged permission redeem script length (only if exits occurred).
    pub perm_redeem_script_len: Option<i64>,
}

/// Successful proof output.
pub struct ProveOutput {
    /// The full receipt (contains journal + inner proof).
    pub receipt: Receipt,
    /// Proving statistics (segments, cycles).
    pub stats: risc0_zkvm::SessionStats,
    /// Elapsed wall-clock time in milliseconds.
    pub elapsed_ms: u128,
}

/// Run the risc0 prover and return the proof or an error message.
///
/// This function is blocking and CPU-intensive. Call it from
/// `tokio::task::spawn_blocking` or a dedicated thread.
///
/// Panics inside the prover (e.g. OOM) are caught and returned as `Err`.
pub fn prove(input: &ProveInput, backend: ProverBackend, kind: ProofKind) -> Result<ProveOutput, String> {
    let env = build_env(input).map_err(|e| format!("Failed to build executor env: {e}"))?;
    let prover = get_prover(backend)?;

    let opts = match kind {
        ProofKind::Succinct => ProverOpts::succinct(),
        ProofKind::Groth16 => ProverOpts::groth16(),
    };

    let now = std::time::Instant::now();
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prover.prove_with_opts(env, ZK_COVENANT_ROLLUP_GUEST_ELF, &opts)));
    let elapsed_ms = now.elapsed().as_millis();

    match result {
        Ok(Ok(info)) => Ok(ProveOutput { receipt: info.receipt, stats: info.stats, elapsed_ms }),
        Ok(Err(e)) => Err(format!("Proving failed: {e}")),
        Err(panic) => {
            let msg = panic
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            Err(format!("Prover panicked: {msg}"))
        }
    }
}

/// Compute the permission redeem script length for a set of exit leaves.
///
/// Returns `None` if `perm_count == 0` (no exits).
pub fn compute_perm_redeem_script_len(perm_root: &[u32; 8], perm_count: u32) -> Option<i64> {
    if perm_count == 0 {
        return None;
    }
    let depth = zk_covenant_rollup_core::permission_tree::required_depth(perm_count as usize);
    let padded_root = zk_covenant_rollup_core::permission_tree::pad_to_depth(*perm_root, perm_count, depth);
    let redeem = zk_covenant_rollup_core::permission_script::build_permission_redeem_bytes_converged(
        &padded_root,
        perm_count as u64,
        depth,
        zk_covenant_rollup_core::MAX_DELEGATE_INPUTS,
    );
    Some(redeem.len() as i64)
}

fn get_prover(backend: ProverBackend) -> Result<Rc<dyn Prover>, String> {
    match backend {
        ProverBackend::Local => Ok(Rc::new(LocalProver::new("local"))),
        ProverBackend::Ipc => {
            let r0vm_path = find_r0vm()?;
            Ok(Rc::new(ExternalProver::new("ipc", r0vm_path)))
        }
    }
}

fn find_r0vm() -> Result<std::path::PathBuf, String> {
    if let Ok(path) = std::env::var("RISC0_SERVER_PATH") {
        let p = std::path::PathBuf::from(&path);
        if p.is_file() {
            return Ok(p);
        }
    }
    // Fall back to bare name — OS will resolve via PATH.
    Ok(std::path::PathBuf::from("r0vm"))
}

fn build_env(input: &ProveInput) -> Result<ExecutorEnv<'_>, String> {
    let mut binding = ExecutorEnv::builder();
    let builder =
        binding.write_slice(core::slice::from_ref(&input.public_input)).write_slice(&(input.block_txs.len() as u32).to_le_bytes());

    for txs in &input.block_txs {
        builder.write_slice(&(txs.len() as u32).to_le_bytes());
        for tx in txs {
            tx.write_to_env(builder);
        }
    }

    // Write permission redeem script length if exits occurred
    if let Some(len) = input.perm_redeem_script_len {
        builder.write_slice(&(len as u32).to_le_bytes());
    }

    builder.build().map_err(|e| format!("Failed to build executor env: {e}"))
}
