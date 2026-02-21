use std::rc::Rc;

use risc0_zkvm::{default_prover, ExecutorEnv, ProveInfo, Prover, ProverOpts, Receipt};
use zk_covenant_rollup_core::PublicInput;
use zk_covenant_rollup_methods::ZK_COVENANT_ROLLUP_GUEST_ELF;

use crate::mock_tx::ZkTransaction;

/// Which risc0 prover backend to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProverBackend {
    /// CPU-only local prover. Works everywhere, slower on large chains.
    Cpu,
    /// CUDA GPU prover. Requires the `cuda` feature and an NVIDIA GPU with enough VRAM.
    Cuda,
    /// IPC prover — communicates with an external `r0vm` subprocess.
    Ipc,
}

impl ProverBackend {
    pub fn label(&self) -> &'static str {
        match self {
            ProverBackend::Cpu => "CPU (local)",
            ProverBackend::Cuda => "CUDA (GPU)",
            ProverBackend::Ipc => "IPC (external r0vm)",
        }
    }

    pub fn all() -> &'static [ProverBackend] {
        &[ProverBackend::Cpu, ProverBackend::Cuda, ProverBackend::Ipc]
    }

    pub fn next(self) -> Self {
        match self {
            ProverBackend::Cpu => ProverBackend::Cuda,
            ProverBackend::Cuda => ProverBackend::Ipc,
            ProverBackend::Ipc => ProverBackend::Cpu,
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
pub fn prove(input: &ProveInput, backend: ProverBackend, kind: ProofKind) -> Result<ProveOutput, String> {
    // Build the executor environment
    let env = build_env(input).map_err(|e| format!("Failed to build executor env: {e}"))?;

    // Select prover backend
    let prover = get_prover(backend)?;

    let opts = match kind {
        ProofKind::Succinct => ProverOpts::succinct(),
        ProofKind::Groth16 => ProverOpts::groth16(),
    };

    let now = std::time::Instant::now();
    let info: ProveInfo =
        prover.prove_with_opts(env, ZK_COVENANT_ROLLUP_GUEST_ELF, &opts).map_err(|e| format!("Proving failed: {e}"))?;
    let elapsed_ms = now.elapsed().as_millis();

    Ok(ProveOutput { receipt: info.receipt, stats: info.stats, elapsed_ms })
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
    // SAFETY: env vars set before prover creation in a single-threaded context
    // (called from spawn_blocking, only one prove runs at a time).
    match backend {
        ProverBackend::Cpu => {
            std::env::set_var("RISC0_PROVER", "local");
            let prover = default_prover();
            std::env::remove_var("RISC0_PROVER");
            Ok(prover)
        }
        ProverBackend::Cuda => get_cuda_prover(),
        ProverBackend::Ipc => {
            std::env::set_var("RISC0_PROVER", "ipc");
            let prover = default_prover();
            std::env::remove_var("RISC0_PROVER");
            Ok(prover)
        }
    }
}

#[cfg(feature = "cuda")]
fn get_cuda_prover() -> Result<Rc<dyn Prover>, String> {
    Ok(default_prover())
}

#[cfg(not(feature = "cuda"))]
fn get_cuda_prover() -> Result<Rc<dyn Prover>, String> {
    Err("CUDA prover requires the `cuda` feature. Rebuild with: cargo build --features cuda".to_string())
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
