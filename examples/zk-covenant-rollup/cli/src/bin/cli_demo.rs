use anyhow::{bail, Context, Result};
use clap::Parser;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::config::params::TESTNET12_PARAMS;
use kaspa_consensus_core::constants::TX_VERSION_POST_COV_HF;
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::sign::{sign, sign_input};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{
    CovenantBinding, ScriptPublicKey, SignableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
    UtxoEntry,
};
use kaspa_hashes::Hash;
use kaspa_rpc_core::{GetBlockDagInfoResponse, RpcTransaction};
use kaspa_txscript::script_builder::ScriptBuilder;
use kaspa_txscript::zk_precompiles::tags::ZkTag;
use kaspa_txscript::{pay_to_address_script, pay_to_script_hash_script};
use kaspa_wrpc_client::prelude::{NetworkId, NetworkType, Notification};
use risc0_zkvm::sha::Digestible;
use zk_covenant_rollup_core::state::empty_tree_root;
use zk_covenant_rollup_host::mock_chain::from_bytes;
use zk_covenant_rollup_host::prove::{self as host_prove, ProofKind, ProveOutput, ProverBackend};
use zk_covenant_rollup_host::redeem::build_redeem_script;
use zk_covenant_rollup_methods::ZK_COVENANT_ROLLUP_GUEST_ID;
use zk_covenant_rollup_tui::actions::compute_fee;
use zk_covenant_rollup_tui::node::{KaspaNode, NodeEvent};
use zk_covenant_rollup_tui::prover::RollupProver;

// ── CLI args ──

#[derive(Parser, Debug)]
#[command(name = "cli-demo")]
#[command(about = "Linear CLI for the ZK Covenant Rollup deploy→sync→prove→submit flow")]
struct Args {
    /// wRPC endpoint. Formats: ip, ip:port, :port, or omitted.
    /// Default IP: 127.0.0.1, default port: 17210
    #[arg(long)]
    rpc: Option<String>,

    /// Deployer private key (64-char hex). If omitted, generates a new keypair.
    #[arg(long)]
    privkey: Option<String>,

    /// Proof type: "succinct" (default) or "groth16"
    #[arg(long, default_value = "succinct")]
    proof_kind: String,

    /// Prover backend: "ipc" (default) or "local"
    #[arg(long, default_value = "ipc")]
    backend: String,

    /// Covenant value in sompi for the deploy transaction.
    #[arg(long, default_value = "100000000")]
    covenant_value: u64,
}

// ── Data types passed between phases ──

struct Keypair {
    secret_key: secp256k1::SecretKey,
    address: Address,
    deployer_spk: ScriptPublicKey,
}

struct DeployResult {
    tx_id: Hash,
    on_chain_covenant_id: Hash,
    starting_block: Hash,
    initial_seq: Hash,
    output_value: u64,
}

struct ProveResult {
    output: ProveOutput,
    block_prove_to: Hash,
    prev_state_hash: [u32; 8],
    prev_seq_commitment: [u32; 8],
}

// ── Arg parsing helpers ──

fn parse_rpc(input: Option<&str>) -> String {
    match input {
        None | Some("") => "ws://127.0.0.1:17210".to_string(),
        Some(s) if s.starts_with("ws://") || s.starts_with("wss://") => s.to_string(),
        Some(s) if s.starts_with(':') => format!("ws://127.0.0.1{s}"),
        Some(s) if s.contains(':') => format!("ws://{s}"),
        Some(s) => format!("ws://{s}:17210"),
    }
}

fn parse_proof_kind(s: &str) -> Result<ProofKind> {
    match s.to_lowercase().as_str() {
        "succinct" | "stark" => Ok(ProofKind::Succinct),
        "groth16" | "snark" => Ok(ProofKind::Groth16),
        _ => bail!("Unknown proof kind: {s} (expected 'succinct' or 'groth16')"),
    }
}

fn parse_backend(s: &str) -> Result<ProverBackend> {
    match s.to_lowercase().as_str() {
        "ipc" => Ok(ProverBackend::Ipc),
        "local" => Ok(ProverBackend::Local),
        _ => bail!("Unknown backend: {s} (expected 'ipc' or 'local')"),
    }
}

// ── Transaction / script helpers ──

fn proof_kind_to_zk_tag(kind: ProofKind) -> ZkTag {
    match kind {
        ProofKind::Succinct => ZkTag::R0Succinct,
        ProofKind::Groth16 => ZkTag::Groth16,
    }
}

fn tx_to_rpc(tx: Transaction) -> RpcTransaction {
    RpcTransaction {
        version: tx.version,
        inputs: tx.inputs.into_iter().map(Into::into).collect(),
        outputs: tx.outputs.into_iter().map(Into::into).collect(),
        lock_time: tx.lock_time,
        subnetwork_id: tx.subnetwork_id,
        gas: tx.gas,
        payload: tx.payload,
        mass: 0,
        verbose_data: None,
    }
}

/// Converge on redeem script length (it encodes its own length, so iterate).
fn converged_redeem_script(
    prev_state_hash: [u32; 8],
    prev_seq_commitment: [u32; 8],
    program_id: &[u8; 32],
    zk_tag: &ZkTag,
) -> Vec<u8> {
    let mut computed_len: i64 = 75;
    loop {
        let script = build_redeem_script(prev_state_hash, prev_seq_commitment, computed_len, program_id, zk_tag);
        let new_len = script.len() as i64;
        if new_len == computed_len {
            return script;
        }
        computed_len = new_len;
    }
}

fn build_sig_script(
    receipt: &risc0_zkvm::Receipt,
    proof_kind: ProofKind,
    block_prove_to: Hash,
    new_state_hash: &[u32; 8],
    input_redeem: &[u8],
) -> Result<Vec<u8>> {
    match proof_kind {
        ProofKind::Succinct => {
            let succinct = receipt.inner.succinct().map_err(|e| anyhow::anyhow!("Not a succinct receipt: {e}"))?;
            let seal_bytes: Vec<u8> = succinct.seal.iter().flat_map(|w| w.to_le_bytes()).collect();
            let claim_bytes: Vec<u8> = succinct.claim.digest().as_bytes().to_vec();
            let hashfn_byte: Vec<u8> =
                vec![zk_covenant_common::hashfn_str_to_id(&succinct.hashfn).ok_or_else(|| anyhow::anyhow!("invalid hashfn"))?];
            let control_index_bytes: Vec<u8> = succinct.control_inclusion_proof.index.to_le_bytes().to_vec();
            let control_digests_bytes: Vec<u8> =
                succinct.control_inclusion_proof.digests.iter().flat_map(|d| d.as_bytes()).copied().collect();
            Ok(ScriptBuilder::new()
                .add_data(&seal_bytes)
                .unwrap()
                .add_data(&claim_bytes)
                .unwrap()
                .add_data(&hashfn_byte)
                .unwrap()
                .add_data(&control_index_bytes)
                .unwrap()
                .add_data(&control_digests_bytes)
                .unwrap()
                .add_data(block_prove_to.as_bytes().as_slice())
                .unwrap()
                .add_data(bytemuck::bytes_of(new_state_hash))
                .unwrap()
                .add_data(input_redeem)
                .unwrap()
                .drain())
        }
        ProofKind::Groth16 => {
            let groth16 = receipt.inner.groth16().map_err(|e| anyhow::anyhow!("Not a groth16 receipt: {e}"))?;
            let compressed_proof = zk_covenant_common::seal_to_compressed_proof(&groth16.seal);
            Ok(ScriptBuilder::new()
                .add_data(&compressed_proof)
                .unwrap()
                .add_data(block_prove_to.as_bytes().as_slice())
                .unwrap()
                .add_data(bytemuck::bytes_of(new_state_hash))
                .unwrap()
                .add_data(input_redeem)
                .unwrap()
                .drain())
        }
    }
}

// ── Phase functions ──

async fn connect(url: &str, network_id: NetworkId) -> Result<(KaspaNode, GetBlockDagInfoResponse)> {
    let node = KaspaNode::try_new(url, network_id).context("Failed to create KaspaNode")?;
    node.connect().await.context("Failed to connect to node")?;

    // Drain the Connected event so it doesn't confuse later listeners
    let receiver = node.event_receiver();
    loop {
        let event = receiver.recv().await.context("Event channel closed")?;
        if matches!(event, NodeEvent::Connected) {
            break;
        }
    }

    let dag_info = node.get_block_dag_info().await.context("get_block_dag_info failed")?;
    println!(
        "  Connected. Network: {}, pruning_point: {}, DAA score: {}",
        dag_info.network, dag_info.pruning_point_hash, dag_info.virtual_daa_score
    );
    Ok((node, dag_info))
}

fn setup_keypair(privkey: Option<&str>, prefix: Prefix) -> Result<Keypair> {
    let secret_key = if let Some(hex_str) = privkey {
        let mut buf = [0u8; 32];
        faster_hex::hex_decode(hex_str.as_bytes(), &mut buf).context("Invalid hex for --privkey")?;
        secp256k1::SecretKey::from_slice(&buf).context("Invalid private key")?
    } else {
        secp256k1::SecretKey::new(&mut rand::thread_rng())
    };

    let public_key = secret_key.public_key(secp256k1::SECP256K1);
    let (xonly_pk, _) = public_key.x_only_public_key();
    let address = Address::new(prefix, Version::PubKey, &xonly_pk.serialize());
    let deployer_spk = pay_to_address_script(&address);

    let mut sk_hex = [0u8; 64];
    faster_hex::hex_encode(&secret_key.secret_bytes(), &mut sk_hex).unwrap();
    println!("  Address:     {address}");
    println!("  Private key: {}", std::str::from_utf8(&sk_hex).unwrap());

    Ok(Keypair { secret_key, address, deployer_spk })
}

async fn wait_for_mature_utxo(node: &KaspaNode, address: &Address, daa_score: u64, min_value: u64) -> Result<(Hash, u32, u64)> {
    let utxos = node.get_utxos_by_addresses(vec![address.clone()]).await.context("get_utxos_by_addresses failed")?;

    if let Some(u) = utxos.iter().find(|u| {
        let age = daa_score.saturating_sub(u.utxo_entry.block_daa_score);
        age >= TESTNET12_PARAMS.coinbase_maturity && u.utxo_entry.amount >= min_value
    }) {
        println!("  Found mature UTXO: {} sompi (age: {} DAA)", u.utxo_entry.amount, daa_score - u.utxo_entry.block_daa_score);
        return Ok((u.outpoint.transaction_id, u.outpoint.index, u.utxo_entry.amount));
    }

    println!("  No mature UTXOs found. Waiting for mature UTXOs at {address} ...");
    println!("  (need >= {min_value} sompi, maturity >= {} DAA blocks)", TESTNET12_PARAMS.coinbase_maturity());

    node.subscribe_utxos(vec![address.clone()]).await.context("subscribe_utxos failed")?;

    let receiver = node.event_receiver();
    let mut current_daa = daa_score;
    loop {
        let event = receiver.recv().await.context("Event channel closed while waiting for UTXOs")?;
        match event {
            NodeEvent::Notification(Notification::VirtualDaaScoreChanged(n)) => {
                current_daa = n.virtual_daa_score;
            }
            NodeEvent::Notification(Notification::UtxosChanged(_)) => {
                let utxos = node.get_utxos_by_addresses(vec![address.clone()]).await.context("get_utxos_by_addresses failed")?;
                if let Some(u) = utxos.iter().find(|u| {
                    let age = current_daa.saturating_sub(u.utxo_entry.block_daa_score);
                    age >= TESTNET12_PARAMS.coinbase_maturity && u.utxo_entry.amount >= min_value
                }) {
                    println!(
                        "  Mature UTXO arrived: {} sompi (age: {} DAA)",
                        u.utxo_entry.amount,
                        current_daa - u.utxo_entry.block_daa_score
                    );
                    return Ok((u.outpoint.transaction_id, u.outpoint.index, u.utxo_entry.amount));
                }
            }
            _ => {}
        }
    }
}

async fn deploy_covenant(
    node: &KaspaNode,
    dag_info: &GetBlockDagInfoResponse,
    keypair: &Keypair,
    proof_kind: ProofKind,
    covenant_value: u64,
    gas_utxo: (Hash, u32, u64),
) -> Result<DeployResult> {
    let (gas_tx_id, gas_index, gas_amount) = gas_utxo;

    // Paginate VCC v1 to find the chain tip
    println!("  Fetching confirmed chain tip from pruning point...");
    let mut current_hash = dag_info.pruning_point_hash;
    let mut last_added_block = None;
    loop {
        let resp = node.get_virtual_chain_from_block(current_hash, false, Some(1000)).await.context("VCC v1 fetch failed")?;
        if resp.added_chain_block_hashes.is_empty() {
            break;
        }
        last_added_block = resp.added_chain_block_hashes.last().copied();
        current_hash = last_added_block.unwrap();
    }
    let starting_block = last_added_block.context("VCC returned no added blocks")?;
    println!("  Deploy starting block: {starting_block}");

    // Get block header for initial seq commitment
    let block = node.get_block(starting_block, false).await.context("get_block failed")?;
    let initial_seq = block.header.accepted_id_merkle_root;
    println!("  Initial seq commitment: {initial_seq}");

    // Build redeem script (convergence loop)
    let prev_state_hash = empty_tree_root();
    let initial_seq_words = from_bytes(initial_seq.as_bytes());
    let program_id: [u8; 32] = bytemuck::cast(ZK_COVENANT_ROLLUP_GUEST_ID);
    let zk_tag = proof_kind_to_zk_tag(proof_kind);

    let redeem_script = converged_redeem_script(prev_state_hash, initial_seq_words, &program_id, &zk_tag);
    let covenant_spk = pay_to_script_hash_script(&redeem_script);
    println!("  Redeem script length: {} bytes", redeem_script.len());

    // Compute on-chain covenant ID
    let deploy_outpoint = TransactionOutpoint::new(gas_tx_id, gas_index);
    let plain_output = TransactionOutput::new(covenant_value, covenant_spk.clone());
    let on_chain_covenant_id =
        kaspa_consensus_core::hashing::covenant_id::covenant_id(deploy_outpoint, std::iter::once((0u32, &plain_output)));
    println!("  On-chain covenant ID: {on_chain_covenant_id}");

    // Estimate fee
    let fee_estimate = node.get_fee_estimate().await.context("get_fee_estimate failed")?;
    let priority_feerate = fee_estimate.priority_bucket.feerate;
    let deploy_fee = compute_fee(3000, priority_feerate);
    println!("  Estimated deploy fee: {deploy_fee} sompi (feerate: {priority_feerate:.2})");

    if gas_amount < covenant_value + deploy_fee {
        bail!("UTXO value {gas_amount} too small for covenant {covenant_value} + fee {deploy_fee}");
    }

    // Build deploy tx
    let inputs = vec![TransactionInput::new(deploy_outpoint, vec![], 0, 0)];
    let utxo_entries = vec![UtxoEntry::new(gas_amount, keypair.deployer_spk.clone(), 0, false, None)];

    let change = gas_amount - covenant_value - deploy_fee;
    let mut outputs = vec![TransactionOutput::with_covenant(
        covenant_value,
        covenant_spk,
        Some(CovenantBinding { covenant_id: on_chain_covenant_id, authorizing_input: 0 }),
    )];
    if change > 0 {
        outputs.push(TransactionOutput::new(change, pay_to_address_script(&keypair.address)));
    }

    let tx = Transaction::new(TX_VERSION_POST_COV_HF, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signable = SignableTransaction::with_entries(tx, utxo_entries);
    let kp = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &keypair.secret_key);
    let signed = sign(signable, kp);

    let tx_id = signed.tx.id();
    println!("  Deploy tx ID: {tx_id}");

    // Submit
    let rpc_tx = tx_to_rpc(signed.tx);
    node.submit_transaction(rpc_tx, false).await.context("Failed to submit deploy tx")?;
    println!("  Deploy tx submitted.");

    Ok(DeployResult { tx_id, on_chain_covenant_id, starting_block, initial_seq, output_value: covenant_value })
}

async fn wait_for_tx_confirmation(node: &KaspaNode, tx_id: Hash) -> Result<()> {
    let receiver = node.event_receiver();
    loop {
        let event = receiver.recv().await.context("Event channel closed while waiting for tx confirmation")?;
        if let NodeEvent::Notification(Notification::VirtualChainChanged(n)) = event {
            for atx in n.accepted_transaction_ids.iter() {
                for id in &atx.accepted_transaction_ids {
                    if *id == tx_id {
                        return Ok(());
                    }
                }
            }
        }
    }
}

async fn sync_chain(node: &KaspaNode, prover: &mut RollupProver) -> Result<()> {
    let mut sync_cursor = prover.last_processed_block;
    let mut total_blocks = 0usize;
    let mut total_txs = 0usize;
    let mut total_actions = 0usize;

    loop {
        let resp = node.get_virtual_chain_v2(sync_cursor, Some(1000)).await.context("VCC v2 fetch failed")?;
        if resp.added_chain_block_hashes.is_empty() {
            break;
        }
        let result = prover.process_chain_response(&resp);
        total_blocks += result.blocks_processed;
        total_txs += result.txs_processed;
        total_actions += result.actions_found;
        sync_cursor = *resp.added_chain_block_hashes.last().unwrap();
    }

    println!("  Synced: {} blocks, {} txs, {} actions", total_blocks, total_txs, total_actions);
    println!("  State root: {}", Hash::from_bytes(bytemuck::cast(prover.state_root)));
    println!("  Seq commitment: {}", prover.seq_commitment);
    println!("  Accumulated blocks for proving: {}", prover.accumulated_blocks());
    Ok(())
}

async fn run_prove(prover: &mut RollupProver, backend: ProverBackend, proof_kind: ProofKind) -> Result<ProveResult> {
    let block_prove_to = prover.last_processed_block;
    let snapshot = prover.take_prove_snapshot().context("No blocks accumulated for proving")?;

    let prev_state_hash = snapshot.input.public_input.prev_state_hash;
    let prev_seq_commitment = snapshot.input.public_input.prev_seq_commitment;

    println!("  Proving {} block(s) with backend={:?}, kind={:?}", snapshot.input.block_txs.len(), backend, proof_kind);
    println!("  block_prove_to: {block_prove_to}");
    println!("  prev_state_hash: {}", Hash::from_bytes(bytemuck::cast(prev_state_hash)));
    println!("  prev_seq_commitment: {}", Hash::from_bytes(bytemuck::cast(prev_seq_commitment)));
    println!("  covenant_id: {}", Hash::from_bytes(bytemuck::cast(snapshot.input.public_input.covenant_id)));

    let output = tokio::task::spawn_blocking(move || host_prove::prove(&snapshot.input, backend, proof_kind))
        .await
        .context("Prove task panicked")?
        .map_err(|e| anyhow::anyhow!("Proving failed: {e}"))?;

    println!("  Proof complete in {:.1}s", output.elapsed_ms as f64 / 1000.0);
    println!("  Stats: {} segments, {} cycles", output.stats.segments, output.stats.total_cycles);
    println!("  Journal length: {} bytes", output.receipt.journal.bytes.len());

    Ok(ProveResult { output, block_prove_to, prev_state_hash, prev_seq_commitment })
}

async fn build_and_submit_proof(
    node: &KaspaNode,
    prove: &ProveResult,
    proof_kind: ProofKind,
    deploy: &DeployResult,
    keypair: &Keypair,
) -> Result<Hash> {
    let journal = &prove.output.receipt.journal.bytes;
    if journal.len() < 128 {
        bail!("Invalid journal length: {} (need >= 128)", journal.len());
    }
    let new_state_hash: [u32; 8] = bytemuck::pod_read_unaligned(&journal[64..96]);
    let new_seq_commitment: [u32; 8] = bytemuck::pod_read_unaligned(&journal[96..128]);

    println!("  New state root:      {}", Hash::from_bytes(bytemuck::cast(new_state_hash)));
    println!("  New seq commitment:  {}", Hash::from_bytes(bytemuck::cast(new_seq_commitment)));

    let program_id: [u8; 32] = bytemuck::cast(ZK_COVENANT_ROLLUP_GUEST_ID);
    let zk_tag = proof_kind_to_zk_tag(proof_kind);

    // Build input and output redeem scripts
    let input_redeem = converged_redeem_script(prove.prev_state_hash, prove.prev_seq_commitment, &program_id, &zk_tag);
    let output_redeem = converged_redeem_script(new_state_hash, new_seq_commitment, &program_id, &zk_tag);
    let output_spk = pay_to_script_hash_script(&output_redeem);

    // Build sig_script for the covenant input
    let sig_script = build_sig_script(&prove.output.receipt, proof_kind, prove.block_prove_to, &new_state_hash, &input_redeem)?;
    println!("  sig_script length: {} bytes", sig_script.len());

    // Find a collateral UTXO from the deployer's address
    let utxos = node.get_utxos_by_addresses(vec![keypair.address.clone()]).await.context("get_utxos_by_addresses failed")?;
    let collateral = utxos
        .iter()
        .find(|u| u.utxo_entry.amount >= 10_000)
        .context("No UTXO available for collateral — fund the deployer address")?;
    let collateral_outpoint = TransactionOutpoint::new(collateral.outpoint.transaction_id, collateral.outpoint.index);
    let collateral_amount = collateral.utxo_entry.amount;
    let collateral_daa = collateral.utxo_entry.block_daa_score;
    println!(
        "  Collateral UTXO: {} sompi (tx: {}:{})",
        collateral_amount, collateral_outpoint.transaction_id, collateral_outpoint.index
    );

    // Build proof transaction: input[0]=covenant, input[1]=collateral
    let covenant_utxo_outpoint = TransactionOutpoint::new(deploy.tx_id, 0);
    let inputs = vec![
        TransactionInput::new(covenant_utxo_outpoint, sig_script, 0, 115),
        TransactionInput::new(collateral_outpoint, vec![], 0, 1),
    ];

    // output[0]=covenant (full value preserved), output[1]=change back to deployer
    let mut outputs = vec![TransactionOutput::with_covenant(
        deploy.output_value,
        output_spk,
        Some(CovenantBinding { authorizing_input: 0, covenant_id: deploy.on_chain_covenant_id }),
    )];
    // Placeholder change output — adjusted after fee estimation
    outputs.push(TransactionOutput::new(collateral_amount, pay_to_address_script(&keypair.address)));

    // Estimate fee from mass
    let tmp_tx = Transaction::new(TX_VERSION_POST_COV_HF, inputs.clone(), outputs.clone(), 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let mass_calc = kaspa_consensus_core::mass::MassCalculator::new(1, 10, 1000, 0);
    let mass = mass_calc.calc_non_contextual_masses(&tmp_tx).compute_mass;
    let fee_estimate = node.get_fee_estimate().await.context("get_fee_estimate failed")?;
    let priority_feerate = fee_estimate.priority_bucket.feerate;
    let estimated_fee = compute_fee(mass, priority_feerate);

    let change = collateral_amount.checked_sub(estimated_fee).context("Collateral UTXO too small to cover fee")?;
    if change > 0 {
        outputs[1].value = change;
    } else {
        outputs.pop(); // no change output if exactly zero
    }
    println!("  Proof tx fee: {estimated_fee} sompi (mass: {mass}, change: {change})");

    // Build the final transaction
    let mut proof_tx = Transaction::new(TX_VERSION_POST_COV_HF, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let proof_tx_id = proof_tx.id();

    // Sign only input[1] (collateral) — input[0] already has the ZK sig_script
    let covenant_entry = UtxoEntry::new(deploy.output_value, pay_to_script_hash_script(&input_redeem), 0, true, None);
    let collateral_entry = UtxoEntry::new(collateral_amount, keypair.deployer_spk.clone(), collateral_daa, false, None);
    let signable = SignableTransaction::with_entries(proof_tx.clone(), vec![covenant_entry, collateral_entry]);
    let collateral_sig = sign_input(&signable.as_verifiable(), 1, &keypair.secret_key.secret_bytes(), SIG_HASH_ALL);
    proof_tx.inputs[1].signature_script = collateral_sig;

    println!("  Proof tx ID: {proof_tx_id}");

    // Submit
    let rpc_tx = tx_to_rpc(proof_tx);
    node.submit_transaction(rpc_tx, false).await.context("Failed to submit proof tx")?;
    println!("  Proof tx submitted.");

    Ok(proof_tx_id)
}

// ── Main ──

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let proof_kind = parse_proof_kind(&args.proof_kind)?;
    let backend = parse_backend(&args.backend)?;
    let url = parse_rpc(args.rpc.as_deref());

    println!("Phase 1: Connecting to {url} ...");
    let (node, dag_info) = connect(&url, NetworkId::with_suffix(NetworkType::Testnet, 12)).await?;

    println!("\nPhase 2: Setting up keypair...");
    let keypair = setup_keypair(args.privkey.as_deref(), Prefix::Testnet)?;

    println!("\nPhase 3: Checking for mature UTXOs...");
    let min_value = args.covenant_value + 10_000;
    let gas_utxo = wait_for_mature_utxo(&node, &keypair.address, dag_info.virtual_daa_score, min_value).await?;

    let dag_info = node.get_block_dag_info().await.context("get_block_dag_info failed")?;

    println!("\nPhase 4: Deploying covenant...");
    let deploy = deploy_covenant(&node, &dag_info, &keypair, proof_kind, args.covenant_value, gas_utxo).await?;

    println!("\nPhase 5: Waiting for deploy tx confirmation...");
    wait_for_tx_confirmation(&node, deploy.tx_id).await?;
    println!("  Deploy tx confirmed!");

    println!("\nPhase 6: Syncing chain for proving...");
    let mut prover = RollupProver::new(deploy.on_chain_covenant_id, empty_tree_root(), deploy.initial_seq, deploy.starting_block);
    sync_chain(&node, &mut prover).await?;

    println!("\nPhase 7: Proving...");
    let prove = run_prove(&mut prover, backend, proof_kind).await?;

    println!("\nPhase 8: Building proof transaction...");
    let proof_tx_id = build_and_submit_proof(&node, &prove, proof_kind, &deploy, &keypair).await?;

    println!("\nPhase 9: Waiting for proof tx confirmation...");
    wait_for_tx_confirmation(&node, proof_tx_id).await?;
    println!("  Proof tx confirmed!");

    println!("\n=== SUCCESS ===");
    println!("  Covenant ID:  {}", deploy.on_chain_covenant_id);
    println!("  Deploy tx:    {}", deploy.tx_id);
    println!("  Proof tx:     {proof_tx_id}");

    node.stop().await.context("Failed to stop node")?;
    Ok(())
}
