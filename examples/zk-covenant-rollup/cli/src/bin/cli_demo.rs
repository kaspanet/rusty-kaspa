use anyhow::{bail, Context, Result};
use clap::Parser;
use kaspa_addresses::{Address, Prefix, Version};
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
use zk_covenant_rollup_core::permission_tree::{perm_empty_leaf_hash, PermissionTree};
use zk_covenant_rollup_core::seq_commit::{calc_accepted_id_merkle_root, seq_commitment_leaf, StreamingMerkleBuilder};
use zk_covenant_rollup_core::state::empty_tree_root;
use zk_covenant_rollup_host::bridge::{build_delegate_entry_script, build_permission_redeem_converged, build_permission_sig_script};
use zk_covenant_rollup_host::mock_chain::{from_bytes, MockSeqCommitAccessor};
use zk_covenant_rollup_host::prove::{self as host_prove, ProofKind, ProveOutput, ProverBackend};
use zk_covenant_rollup_host::redeem::build_redeem_script;
use zk_covenant_rollup_host::tx::try_verify_tx_input;
use zk_covenant_rollup_methods::ZK_COVENANT_ROLLUP_GUEST_ID;
use zk_covenant_rollup_tui::actions::{build_entry_tx, build_exit_tx, build_transfer_tx, compute_fee};
use zk_covenant_rollup_tui::balance::Utxo;
use zk_covenant_rollup_tui::db::RollupDb;
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

    /// Minimum UTXO maturity in DAA blocks (coinbase maturity).
    /// Lower values speed up the demo but may cause issues on mainnet.
    #[arg(long, default_value = "1000")]
    maturity: u64,
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
    deploy_change: u64,
}

struct ProveResult {
    output: ProveOutput,
    block_prove_to: Hash,
    prev_state_hash: [u32; 8],
    prev_seq_commitment: [u32; 8],
    perm_redeem_script: Option<Vec<u8>>,
    perm_exit_data: Vec<(Vec<u8>, u64)>,
}

struct WithdrawResult {
    tx_id: Hash,
    continuation: Option<WithdrawContinuation>,
}

struct WithdrawContinuation {
    perm_redeem: Vec<u8>,
    exit_data: Vec<(Vec<u8>, u64)>,
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
        "local" => {
            #[cfg(feature = "cuda")]
            {
                Ok(ProverBackend::Local)
            }
            #[cfg(not(feature = "cuda"))]
            bail!("Local prover backend requires CUDA support")
        }
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

async fn wait_for_mature_utxo(
    node: &KaspaNode,
    address: &Address,
    daa_score: u64,
    min_value: u64,
    maturity: u64,
) -> Result<(Hash, u32, u64)> {
    let utxos = node.get_utxos_by_addresses(vec![address.clone()]).await.context("get_utxos_by_addresses failed")?;

    if let Some(u) = utxos.iter().find(|u| {
        let age = daa_score.saturating_sub(u.utxo_entry.block_daa_score);
        age >= maturity && u.utxo_entry.amount >= min_value
    }) {
        println!("  Found mature UTXO: {} sompi (age: {} DAA)", u.utxo_entry.amount, daa_score - u.utxo_entry.block_daa_score);
        return Ok((u.outpoint.transaction_id, u.outpoint.index, u.utxo_entry.amount));
    }

    println!("  No mature UTXOs found. Waiting for mature UTXOs at {address} ...");
    println!("  (need >= {min_value} sompi, maturity >= {maturity} DAA blocks)");

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
                    age >= maturity && u.utxo_entry.amount >= min_value
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

    Ok(DeployResult { tx_id, on_chain_covenant_id, starting_block, initial_seq, output_value: covenant_value, deploy_change: change })
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

async fn sync_chain(node: &KaspaNode, prover: &mut RollupProver) -> Result<usize> {
    let mut sync_cursor = prover.last_processed_block;
    let mut total_blocks = 0usize;
    let mut total_txs = 0usize;
    let mut total_actions = 0usize;
    let mut seq_mismatches = 0usize;

    loop {
        let resp = node.get_virtual_chain_v2(sync_cursor, Some(1000)).await.context("VCC v2 fetch failed")?;
        if resp.added_chain_block_hashes.is_empty() {
            break;
        }

        // Capture seq commitment BEFORE this batch for independent re-computation.
        let mut computed_seq: [u32; 8] = from_bytes(prover.seq_commitment.as_bytes());

        let result = prover.process_chain_response(&resp);

        // Independently re-derive seq commitment per block using the same streaming merkle
        // that the guest uses: seq_commitment_leaf(tx_id, version) for each ZkTransaction.
        // If our computed AIR matches block header AIR → rollup impl is correct, guest needs debugging.
        // If it doesn't match → tx_id reconstruction or tx filtering has a bug in the rollup impl.
        for (i, (block_zk_txs, rpc_block)) in
            prover.last_block_txs.iter().zip(resp.chain_block_accepted_transactions.iter()).enumerate()
        {
            let rpc_tx_count = rpc_block.accepted_transactions.len();
            let zk_tx_count = block_zk_txs.len();
            if rpc_tx_count != zk_tx_count {
                eprintln!(
                    "  [WARN] block[{i}]: RPC has {rpc_tx_count} txs but ZkTransactions has {zk_tx_count} \
                     (some dropped by rpc_optional_to_transaction)"
                );
            }

            let mut builder = StreamingMerkleBuilder::new();
            for zk_tx in block_zk_txs {
                builder.add_leaf(seq_commitment_leaf(&zk_tx.tx_id(), zk_tx.version()));
            }
            let block_root = builder.finalize();
            computed_seq = calc_accepted_id_merkle_root(&computed_seq, &block_root);
            let computed_hash = Hash::from_bytes(bytemuck::cast(computed_seq));

            let block_hash = resp.added_chain_block_hashes.get(i).copied().unwrap_or_default();
            match rpc_block.chain_block_header.accepted_id_merkle_root {
                Some(header_air) if computed_hash != header_air => {
                    seq_mismatches += 1;
                    eprintln!(
                        "  [SEQ MISMATCH] block[{i}] {block_hash} ({zk_tx_count}/{rpc_tx_count} ZkTxs/RPC)\n    \
                         computed: {computed_hash}\n    header:   {header_air}"
                    );
                }
                None => {
                    eprintln!("  [WARN] block[{i}] {block_hash} has no AIR in header");
                }
                _ => {}
            }
        }

        total_blocks += result.blocks_processed;
        total_txs += result.txs_processed;
        total_actions += result.actions_found;
        sync_cursor = *resp.added_chain_block_hashes.last().unwrap();
    }

    if seq_mismatches > 0 {
        bail!(
            "{seq_mismatches} seq commitment mismatch(es) during sync — \
             tx_id reconstruction or tx filtering bug in rollup impl"
        );
    }

    println!("  Synced: {} blocks, {} txs, {} actions", total_blocks, total_txs, total_actions);
    println!("  [ok] All {total_blocks} blocks verified: computed seq commitments match block headers");
    println!("  State root: {}", Hash::from_bytes(bytemuck::cast(prover.state_root)));
    println!("  Seq commitment: {}", prover.seq_commitment);
    println!("  Accumulated blocks for proving: {}", prover.accumulated_blocks());
    Ok(total_actions)
}

async fn run_prove(prover: &mut RollupProver, backend: ProverBackend, proof_kind: ProofKind) -> Result<ProveResult> {
    let block_prove_to = prover.last_processed_block;
    let snapshot = prover.take_prove_snapshot().context("No blocks accumulated for proving")?;

    let prev_state_hash = snapshot.input.public_input.prev_state_hash;
    let prev_seq_commitment = snapshot.input.public_input.prev_seq_commitment;
    let perm_redeem_script = snapshot.perm_redeem_script;
    let perm_exit_data = snapshot.perm_exit_data;
    let input = snapshot.input;

    println!("  Proving {} block(s) with backend={:?}, kind={:?}", input.block_txs.len(), backend, proof_kind);
    println!("  block_prove_to: {block_prove_to}");
    println!("  prev_state_hash: {}", Hash::from_bytes(bytemuck::cast(prev_state_hash)));
    println!("  prev_seq_commitment: {}", Hash::from_bytes(bytemuck::cast(prev_seq_commitment)));
    println!("  covenant_id: {}", Hash::from_bytes(bytemuck::cast(input.public_input.covenant_id)));
    if perm_redeem_script.is_some() {
        println!("  Permission tree: {} exit leaves", perm_exit_data.len());
    }

    let output = tokio::task::spawn_blocking(move || host_prove::prove(&input, backend, proof_kind))
        .await
        .context("Prove task panicked")?
        .map_err(|e| anyhow::anyhow!("Proving failed: {e}"))?;

    println!("  Proof complete in {:.1}s", output.elapsed_ms as f64 / 1000.0);
    println!("  Stats: {} segments, {} cycles", output.stats.segments, output.stats.total_cycles);
    println!("  Journal length: {} bytes", output.receipt.journal.bytes.len());

    Ok(ProveResult { output, block_prove_to, prev_state_hash, prev_seq_commitment, perm_redeem_script, perm_exit_data })
}

async fn build_and_submit_proof(
    node: &KaspaNode,
    prove: &ProveResult,
    proof_kind: ProofKind,
    deploy: &DeployResult,
    keypair: &Keypair,
) -> Result<Hash> {
    let journal = &prove.output.receipt.journal.bytes;
    if journal.len() < 160 {
        bail!("Invalid journal length: {} (need >= 160)", journal.len());
    }

    // Extract all journal fields
    let journal_prev_seq: [u32; 8] = bytemuck::pod_read_unaligned(&journal[32..64]);
    let new_state_hash: [u32; 8] = bytemuck::pod_read_unaligned(&journal[64..96]);
    let new_seq_commitment: [u32; 8] = bytemuck::pod_read_unaligned(&journal[96..128]);
    let journal_covenant_id: [u32; 8] = bytemuck::pod_read_unaligned(&journal[128..160]);
    let new_seq_hash = Hash::from_bytes(bytemuck::cast(new_seq_commitment));

    println!("  New state root:      {}", Hash::from_bytes(bytemuck::cast(new_state_hash)));
    println!("  New seq commitment:  {new_seq_hash}");

    // ── Journal field verification ───────────────────────────────────────────
    // 1. prev_seq_commitment must match what was passed to the prover
    if journal_prev_seq != prove.prev_seq_commitment {
        bail!(
            "Journal prev_seq_commitment mismatch\n  journal:  {}\n  expected: {}",
            Hash::from_bytes(bytemuck::cast(journal_prev_seq)),
            Hash::from_bytes(bytemuck::cast(prove.prev_seq_commitment)),
        );
    }
    println!("  [ok] prev_seq_commitment matches");

    // 2. covenant_id in journal must match deploy
    let expected_cov_id: [u32; 8] = from_bytes(deploy.on_chain_covenant_id.as_bytes());
    if journal_covenant_id != expected_cov_id {
        bail!(
            "Journal covenant_id mismatch\n  journal:  {}\n  expected: {}",
            Hash::from_bytes(bytemuck::cast(journal_covenant_id)),
            deploy.on_chain_covenant_id,
        );
    }
    println!("  [ok] covenant_id matches");

    // 3. new_seq_commitment must equal block_prove_to.accepted_id_merkle_root
    //    (OpSeqCommit checks this exact equality on-chain)
    let block_prove_block =
        node.get_block(prove.block_prove_to, false).await.context("get block_prove_to for seq verification failed")?;
    let block_air = block_prove_block.header.accepted_id_merkle_root;
    if new_seq_hash != block_air {
        bail!(
            "Journal new_seq_commitment != block_prove_to.accepted_id_merkle_root\n  journal:        {}\n  block AIR:      {}\n  block_prove_to: {}",
            new_seq_hash,
            block_air,
            prove.block_prove_to,
        );
    }
    println!("  [ok] new_seq_commitment matches block_prove_to accepted_id_merkle_root");

    // ── On-chain journal preimage verification ──────────────────────────────
    // Reconstruct the preimage exactly as the on-chain script would build it
    // via OpFromAltStack + OpInputCovenantId + OpCovOutCount introspection,
    // then compare with the guest's actual journal bytes.
    {
        let journal_prev_state: [u32; 8] = bytemuck::pod_read_unaligned(&journal[0..32]);
        let mut onchain_preimage = Vec::with_capacity(192);
        onchain_preimage.extend_from_slice(bytemuck::bytes_of(&journal_prev_state)); // prev_state
        onchain_preimage.extend_from_slice(bytemuck::bytes_of(&journal_prev_seq)); // prev_seq
        onchain_preimage.extend_from_slice(bytemuck::bytes_of(&new_state_hash)); // new_state
        onchain_preimage.extend_from_slice(bytemuck::bytes_of(&new_seq_commitment)); // new_seq
        onchain_preimage.extend_from_slice(&deploy.on_chain_covenant_id.as_bytes()); // covenant_id

        if let Some(ref perm_redeem) = prove.perm_redeem_script {
            // On-chain: extracts blake2b hash from output[1] P2SH SPK to_bytes()[4..36]
            // to_bytes() = version_u16_le(2) || script(35), so [4..36] = script[2..34] = the hash
            let perm_spk = pay_to_script_hash_script(perm_redeem);
            let perm_hash = &perm_spk.script()[2..34];
            onchain_preimage.extend_from_slice(perm_hash);

            // Also check what the guest committed at journal[160..192]
            if journal.len() >= 192 {
                let journal_perm_hash = &journal[160..192];
                if perm_hash != journal_perm_hash {
                    println!("  [MISMATCH] permission_spk_hash:");
                    println!("    on-chain SPK[4..36]: {}", faster_hex::hex_string(perm_hash));
                    println!("    journal[160..192]:   {}", faster_hex::hex_string(journal_perm_hash));
                } else {
                    println!("  [ok] permission_spk_hash matches ({} bytes)", perm_hash.len());
                }
            } else {
                println!("  [MISMATCH] permission output present but journal only {} bytes (expected 192)", journal.len());
            }
        }

        if onchain_preimage.as_slice() != journal.as_slice() {
            println!("  [FAIL] On-chain preimage != journal bytes!");
            println!("    on-chain preimage len: {}", onchain_preimage.len());
            println!("    journal len:           {}", journal.len());
            for (i, (a, b)) in onchain_preimage.iter().zip(journal.iter()).enumerate() {
                if a != b {
                    println!("    first diff at byte {i}: on-chain=0x{a:02x} journal=0x{b:02x}");
                    break;
                }
            }
            bail!("On-chain journal preimage does not match guest journal — ZK verification will fail");
        }
        println!("  [ok] on-chain journal preimage matches guest journal ({} bytes)", journal.len());
    }
    // ────────────────────────────────────────────────────────────────────────

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
    let has_permission = prove.perm_redeem_script.is_some();
    let min_collateral = if has_permission { deploy.output_value + 10_000 } else { 10_000 };
    let rpc_utxos = node.get_utxos_by_addresses(vec![keypair.address.clone()]).await.context("get_utxos_by_addresses failed")?;
    let collateral = rpc_utxos
        .iter()
        .find(|u| u.utxo_entry.amount >= min_collateral)
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

    // output[0]=covenant (full value preserved)
    let mut outputs = vec![TransactionOutput::with_covenant(
        deploy.output_value,
        output_spk,
        Some(CovenantBinding { authorizing_input: 0, covenant_id: deploy.on_chain_covenant_id }),
    )];

    // output[1]=permission (if exits occurred in this batch)
    if let Some(ref perm_redeem) = prove.perm_redeem_script {
        let perm_spk = pay_to_script_hash_script(perm_redeem);
        outputs.push(TransactionOutput::with_covenant(
            deploy.output_value,
            perm_spk,
            Some(CovenantBinding { authorizing_input: 0, covenant_id: deploy.on_chain_covenant_id }),
        ));
        println!("  Permission output: {} sompi ({} exit leaves)", deploy.output_value, prove.perm_exit_data.len());
    }

    // Placeholder change output — adjusted after fee estimation
    outputs.push(TransactionOutput::new(collateral_amount, pay_to_address_script(&keypair.address)));

    // Estimate fee from mass
    let tmp_tx = Transaction::new(TX_VERSION_POST_COV_HF, inputs.clone(), outputs.clone(), 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let mass_calc = kaspa_consensus_core::mass::MassCalculator::new(1, 10, 1000, 0);
    let mass = mass_calc.calc_non_contextual_masses(&tmp_tx).compute_mass;
    let fee_estimate = node.get_fee_estimate().await.context("get_fee_estimate failed")?;
    let priority_feerate = fee_estimate.priority_bucket.feerate;
    let estimated_fee = compute_fee(mass, priority_feerate);

    let perm_cost = if has_permission { deploy.output_value } else { 0 };
    let change = collateral_amount
        .checked_sub(estimated_fee + perm_cost)
        .context("Collateral UTXO too small to cover fee + permission value")?;
    let change_idx = outputs.len() - 1;
    if change > 0 {
        outputs[change_idx].value = change;
    } else {
        outputs.pop(); // no change output if exactly zero
    }
    println!("  Proof tx fee: {estimated_fee} sompi (mass: {mass}, perm_cost: {perm_cost}, change: {change})");

    // Build the final transaction
    let mut proof_tx = Transaction::new(TX_VERSION_POST_COV_HF, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let proof_tx_id = proof_tx.id();

    // Sign only input[1] (collateral) — input[0] already has the ZK sig_script
    let covenant_entry =
        UtxoEntry::new(deploy.output_value, pay_to_script_hash_script(&input_redeem), 0, false, Some(deploy.on_chain_covenant_id));
    let collateral_entry = UtxoEntry::new(collateral_amount, keypair.deployer_spk.clone(), collateral_daa, false, None);
    let signable = SignableTransaction::with_entries(proof_tx.clone(), vec![covenant_entry.clone(), collateral_entry.clone()]);
    let collateral_sig = sign_input(&signable.as_verifiable(), 1, &keypair.secret_key.secret_bytes(), SIG_HASH_ALL);
    proof_tx.inputs[1].signature_script = collateral_sig;

    // ── Local script verification (same path as on-chain) ───────────────────
    let accessor = MockSeqCommitAccessor(std::collections::HashMap::from([(prove.block_prove_to, new_seq_hash)]));
    match try_verify_tx_input(&proof_tx, &[covenant_entry, collateral_entry], 0, &accessor) {
        Ok(()) => println!("  [ok] Local script verification passed"),
        Err(e) => bail!("Local script verification failed: {e}\n  (the on-chain script will also reject this tx)"),
    }
    // ────────────────────────────────────────────────────────────────────────

    println!("  Proof tx ID: {proof_tx_id}");

    // Submit
    let rpc_tx = tx_to_rpc(proof_tx);
    node.submit_transaction(rpc_tx, false).await.context("Failed to submit proof tx")?;
    println!("  Proof tx submitted.");

    Ok(proof_tx_id)
}

// ── Action / withdrawal helpers ──

fn sign_action_tx(tx: Transaction, secret_key: &secp256k1::SecretKey, sender_spk: &ScriptPublicKey, gas_amount: u64) -> Transaction {
    let utxo_entry = UtxoEntry::new(gas_amount, sender_spk.clone(), 0, false, None);
    let signable = SignableTransaction::with_entries(tx, vec![utxo_entry]);
    let kp = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, secret_key);
    sign(signable, kp).tx
}

fn derive_delegate_address(covenant_id: Hash, prefix: Prefix) -> Address {
    let delegate_script = build_delegate_entry_script(&covenant_id.as_bytes());
    let delegate_spk = pay_to_script_hash_script(&delegate_script);
    let script_bytes = delegate_spk.script();
    let hash_bytes: [u8; 32] = script_bytes[2..34].try_into().unwrap();
    Address::new(prefix, Version::ScriptHash, &hash_bytes)
}

async fn withdraw_exit(
    node: &KaspaNode,
    covenant_id: Hash,
    perm_outpoint: (Hash, u32),
    perm_value: u64,
    perm_redeem: &[u8],
    exit_data: &[(Vec<u8>, u64)],
    leaf_idx: usize,
    delegate_address: &Address,
    keypair: &Keypair,
) -> Result<WithdrawResult> {
    let (ref spk, amount) = exit_data[leaf_idx];
    let unclaimed = exit_data.len() as u64;

    // Build permission tree and proof
    let tree = PermissionTree::from_leaves(exit_data.to_vec());
    let proof = tree.prove(leaf_idx);
    let perm_sig_script = build_permission_sig_script(spk, amount, amount, &proof, perm_redeem);

    // Build delegate entry script and sig_script
    let delegate_script = build_delegate_entry_script(&covenant_id.as_bytes());
    let delegate_spk = pay_to_script_hash_script(&delegate_script);
    let delegate_sig_script = ScriptBuilder::new().add_data(&delegate_script).unwrap().drain();

    // Find delegate UTXOs covering amount
    let rpc_utxos = node.get_utxos_by_addresses(vec![delegate_address.clone()]).await.context("get delegate UTXOs")?;
    let mut selected_delegates: Vec<(Hash, u32, u64)> = Vec::new();
    let mut delegate_total: u64 = 0;
    for u in &rpc_utxos {
        selected_delegates.push((u.outpoint.transaction_id, u.outpoint.index, u.utxo_entry.amount));
        delegate_total += u.utxo_entry.amount;
        if delegate_total >= amount {
            break;
        }
    }
    if delegate_total < amount {
        bail!("Insufficient delegate UTXOs ({delegate_total} < {amount} sompi)");
    }
    println!("  Delegate UTXOs: {} covering {delegate_total} sompi", selected_delegates.len());

    // Find collateral UTXO for fee payment
    let rpc_collateral = node.get_utxos_by_addresses(vec![keypair.address.clone()]).await.context("get collateral UTXOs")?;
    let collateral = rpc_collateral
        .iter()
        .find(|u| u.utxo_entry.amount >= 10_000)
        .context("No collateral UTXO for withdrawal fee — fund the deployer address")?;
    let collateral_outpoint = TransactionOutpoint::new(collateral.outpoint.transaction_id, collateral.outpoint.index);
    let collateral_amount = collateral.utxo_entry.amount;
    let collateral_daa = collateral.utxo_entry.block_daa_score;
    println!(
        "  Collateral UTXO: {collateral_amount} sompi (tx: {}:{})",
        collateral.outpoint.transaction_id, collateral.outpoint.index
    );

    // Estimate fee
    let fee_estimate = node.get_fee_estimate().await.context("get_fee_estimate failed")?;
    let priority_feerate = fee_estimate.priority_bucket.feerate;
    let estimated_fee = compute_fee(5000, priority_feerate);

    // Full withdrawal amount goes to destination — fee paid by collateral
    let dest_value = amount;

    // Build inputs: permission (seq=0) + delegates (seq=0) + collateral (seq=1)
    let mut inputs = vec![TransactionInput::new(TransactionOutpoint::new(perm_outpoint.0, perm_outpoint.1), perm_sig_script, 0, 0)];
    for &(tx_id, index, _) in &selected_delegates {
        inputs.push(TransactionInput::new(TransactionOutpoint::new(tx_id, index), delegate_sig_script.clone(), 0, 0));
    }
    inputs.push(TransactionInput::new(collateral_outpoint, vec![], 0, 1));

    // Build outputs: withdrawal destination
    let dest_spk = ScriptPublicKey::new(0, spk.clone().into());
    let mut outputs = vec![TransactionOutput::new(dest_value, dest_spk)];

    // Continuation permission output (if more exits remain)
    let continuation = if unclaimed > 1 {
        let new_unclaimed = unclaimed - 1;
        let depth = tree.depth();
        let new_root = proof.compute_new_root(&perm_empty_leaf_hash());
        let max_inputs = std::num::NonZeroUsize::new(zk_covenant_rollup_core::MAX_DELEGATE_INPUTS).unwrap();
        let new_redeem = build_permission_redeem_converged(&new_root, new_unclaimed, depth, max_inputs);
        let new_perm_spk = pay_to_script_hash_script(&new_redeem);
        outputs.push(TransactionOutput::with_covenant(
            perm_value,
            new_perm_spk,
            Some(CovenantBinding { authorizing_input: 0, covenant_id }),
        ));
        let mut remaining = exit_data.to_vec();
        remaining[leaf_idx] = (vec![], 0); // mark withdrawn, preserve tree structure
        Some(WithdrawContinuation { perm_redeem: new_redeem, exit_data: remaining })
    } else {
        None
    };

    // Delegate change
    if delegate_total > amount {
        let delegate_change = delegate_total - amount;
        outputs.push(TransactionOutput::new(delegate_change, delegate_spk.clone()));
    }

    // Collateral change
    let collateral_change = collateral_amount.checked_sub(estimated_fee).context("Collateral too small for fee")?;
    if collateral_change > 0 {
        outputs.push(TransactionOutput::new(collateral_change, keypair.deployer_spk.clone()));
    }

    let mut tx = Transaction::new(TX_VERSION_POST_COV_HF, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);

    // Sign collateral input
    let perm_entry = UtxoEntry::new(perm_value, pay_to_script_hash_script(perm_redeem), 0, true, Some(covenant_id));
    let mut all_entries: Vec<UtxoEntry> = vec![perm_entry];
    for &(_, _, amt) in &selected_delegates {
        all_entries.push(UtxoEntry::new(amt, delegate_spk.clone(), 0, false, None));
    }
    all_entries.push(UtxoEntry::new(collateral_amount, keypair.deployer_spk.clone(), collateral_daa, false, None));

    let collateral_input_idx = tx.inputs.len() - 1;
    let signable = SignableTransaction::with_entries(tx.clone(), all_entries.clone());
    let sig = sign_input(&signable.as_verifiable(), collateral_input_idx, &keypair.secret_key.secret_bytes(), SIG_HASH_ALL);
    tx.inputs[collateral_input_idx].signature_script = sig;

    let tx_id = tx.id();
    println!("  Withdraw tx ID: {tx_id}");
    println!("  Destination: {dest_value} sompi (fee: {estimated_fee})");

    // ── Local script verification ────────────────────────────────────────
    {
        println!("  [diag] inputs: {} (perm + {} delegates + collateral)", tx.inputs.len(), selected_delegates.len());
        println!("  [diag] outputs: {}", tx.outputs.len());
        for (i, out) in tx.outputs.iter().enumerate() {
            println!("    output[{i}]: {} sompi, spk_len={}", out.value, out.script_public_key.script().len());
        }
        println!("  [diag] perm_utxo SPK hash: {}", faster_hex::hex_string(&all_entries[0].script_public_key.script()[2..34]));
        println!("  [diag] delegate_total={delegate_total} deduct={amount} change={}", delegate_total - amount);

        let accessor = MockSeqCommitAccessor(std::collections::HashMap::new());
        match try_verify_tx_input(&tx, &all_entries, 0, &accessor) {
            Ok(()) => println!("  [ok] Local permission script verification passed"),
            Err(e) => bail!("Local permission script verification FAILED: {e}\n  (the on-chain script will also reject this tx)"),
        }
    }
    // ────────────────────────────────────────────────────────────────────

    let rpc_tx = tx_to_rpc(tx);
    node.submit_transaction(rpc_tx, false).await.context("Failed to submit withdraw tx")?;
    println!("  Withdraw tx submitted.");

    Ok(WithdrawResult { tx_id, continuation })
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

    // L2 operation amounts
    let deposit_amount: u64 = 50_000_000;
    let transfer_amount: u64 = 20_000_000;
    let exit1_amount: u64 = 15_000_000;
    let exit2_amount: u64 = 20_000_000;

    // Derive deployer's public key hash (Pubkey = Hash in this codebase)
    let deployer_pk = {
        let pk = keypair.secret_key.public_key(secp256k1::SECP256K1);
        let (xonly, _) = pk.x_only_public_key();
        Hash::from_bytes(xonly.serialize())
    };

    println!("\nPhase 3: Checking for mature UTXOs...");
    // Need: covenant_value*2 (deploy covenant + permission output) + deposit + headroom for fees
    let min_value = args.covenant_value * 2 + deposit_amount + 1_000_000;
    let gas_utxo = wait_for_mature_utxo(&node, &keypair.address, dag_info.virtual_daa_score, min_value, args.maturity).await?;

    let dag_info = node.get_block_dag_info().await.context("get_block_dag_info failed")?;

    println!("\nPhase 4: Deploying covenant...");
    let deploy = deploy_covenant(&node, &dag_info, &keypair, proof_kind, args.covenant_value, gas_utxo).await?;

    println!("\nPhase 5: Waiting for deploy tx confirmation...");
    wait_for_tx_confirmation(&node, deploy.tx_id).await?;
    println!("  Deploy tx confirmed!");

    // Get fee estimate for action transactions
    let fee_estimate = node.get_fee_estimate().await.context("get_fee_estimate failed")?;
    let priority_feerate = fee_estimate.priority_bucket.feerate;
    let action_fee = compute_fee(2000, priority_feerate);
    println!("  Action fee: {action_fee} sompi (feerate: {priority_feerate:.2})");

    // ── Phase 6: Init prover ──────────────────────────────────────────────

    println!("\nPhase 6: Initializing prover...");
    let db_path = std::env::temp_dir().join(format!("cli-demo-rollup-db-{}", deploy.on_chain_covenant_id));
    let db = std::sync::Arc::new(RollupDb::open(&db_path).context("open rollup db")?);
    let mut prover = RollupProver::new(deploy.on_chain_covenant_id, empty_tree_root(), deploy.initial_seq, deploy.starting_block, db);
    println!("  Prover initialized. Starting block: {}", deploy.starting_block);

    // ── Phase 7: Entry deposit ────────────────────────────────────────────

    println!("\nPhase 7: Entry deposit ({deposit_amount} sompi to deployer L2 account)...");
    let entry_gas = Utxo { tx_id: deploy.tx_id, index: 1, amount: deploy.deploy_change };
    let entry_tx = build_entry_tx(deployer_pk, deploy.on_chain_covenant_id, deposit_amount, &entry_gas, action_fee)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let entry_tx_id = {
        let signed = sign_action_tx(entry_tx, &keypair.secret_key, &keypair.deployer_spk, entry_gas.amount);
        let tx_id = signed.id();
        println!("  Entry tx ID: {tx_id}");
        node.submit_transaction(tx_to_rpc(signed), false).await.context("submit entry tx")?;
        println!("  Waiting for confirmation...");
        wait_for_tx_confirmation(&node, tx_id).await?;
        println!("  Entry tx confirmed!");
        tx_id
    };
    let entry_change_amount = entry_gas.amount - deposit_amount - action_fee;

    // ── Phase 8: Exit #1 ─────────────────────────────────────────────────

    println!("\nPhase 8: Exit #1 ({exit1_amount} L2 sompi from deployer, dest=deployer)...");
    let exit1_gas = Utxo { tx_id: entry_tx_id, index: 1, amount: entry_change_amount };
    let exit1_dest_spk_bytes = keypair.deployer_spk.script().to_vec();
    let exit1_tx =
        build_exit_tx(deployer_pk, exit1_amount, &exit1_dest_spk_bytes, &exit1_gas, action_fee).map_err(|e| anyhow::anyhow!("{e}"))?;
    let exit1_tx_id = {
        let signed = sign_action_tx(exit1_tx, &keypair.secret_key, &keypair.deployer_spk, exit1_gas.amount);
        let tx_id = signed.id();
        println!("  Exit #1 tx ID: {tx_id}");
        node.submit_transaction(tx_to_rpc(signed), false).await.context("submit exit #1 tx")?;
        println!("  Waiting for confirmation...");
        wait_for_tx_confirmation(&node, tx_id).await?;
        println!("  Exit #1 tx confirmed!");
        tx_id
    };
    let exit1_output_amount = exit1_gas.amount - action_fee;

    // ── Phase 9: Generate keypair #2 ─────────────────────────────────────

    println!("\nPhase 9: Generating keypair #2...");
    let sk2 = secp256k1::SecretKey::new(&mut rand::thread_rng());
    let pk2_full = sk2.public_key(secp256k1::SECP256K1);
    let (xonly_pk2, _) = pk2_full.x_only_public_key();
    let addr2 = Address::new(Prefix::Testnet, Version::PubKey, &xonly_pk2.serialize());
    let spk2 = pay_to_address_script(&addr2);
    let pk2_hash = Hash::from_bytes(xonly_pk2.serialize());
    println!("  Address #2: {addr2}");

    // ── Phase 10: L2 Transfer ────────────────────────────────────────────

    println!("\nPhase 10: Transfer ({transfer_amount} L2 sompi, deployer -> account #2)...");
    let transfer_gas = Utxo { tx_id: exit1_tx_id, index: 0, amount: exit1_output_amount };
    let transfer_tx = build_transfer_tx(deployer_pk, pk2_hash, transfer_amount, &transfer_gas, &addr2, action_fee)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let transfer_tx_id = {
        let signed = sign_action_tx(transfer_tx, &keypair.secret_key, &keypair.deployer_spk, transfer_gas.amount);
        let tx_id = signed.id();
        println!("  Transfer tx ID: {tx_id}");
        node.submit_transaction(tx_to_rpc(signed), false).await.context("submit transfer tx")?;
        println!("  Waiting for confirmation...");
        wait_for_tx_confirmation(&node, tx_id).await?;
        println!("  Transfer tx confirmed!");
        tx_id
    };
    let transfer_output_amount = transfer_gas.amount - action_fee;

    // ── Phase 11: Exit #2 ────────────────────────────────────────────────

    // Exit #2 destination is deployer's address so the output serves as proof collateral
    println!("\nPhase 11: Exit #2 ({exit2_amount} L2 sompi from account #2, dest=deployer)...");
    let exit2_gas = Utxo { tx_id: transfer_tx_id, index: 0, amount: transfer_output_amount };
    let exit2_tx =
        build_exit_tx(pk2_hash, exit2_amount, &exit1_dest_spk_bytes, &exit2_gas, action_fee).map_err(|e| anyhow::anyhow!("{e}"))?;
    let exit2_tx_id = {
        let signed = sign_action_tx(exit2_tx, &sk2, &spk2, exit2_gas.amount);
        let tx_id = signed.id();
        println!("  Exit #2 tx ID: {tx_id}");
        node.submit_transaction(tx_to_rpc(signed), false).await.context("submit exit #2 tx")?;
        println!("  Waiting for confirmation...");
        wait_for_tx_confirmation(&node, tx_id).await?;
        println!("  Exit #2 tx confirmed!");
        tx_id
    };

    // ── Phase 12: Sync chain (retry until all actions found) ─────────────

    println!("\nPhase 12: Syncing chain...");
    let expected_actions = 4; // entry + exit1 + transfer + exit2
    let mut cumulative_actions = 0usize;
    loop {
        let actions = sync_chain(&node, &mut prover).await?;
        cumulative_actions += actions;
        if cumulative_actions >= expected_actions {
            println!("  All {expected_actions} actions found.");
            break;
        }
        println!("  Found {cumulative_actions}/{expected_actions} actions so far, waiting for more blocks...");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    // ── Phase 13: Prove ──────────────────────────────────────────────────

    println!("\nPhase 13: Proving...");
    let prove = run_prove(&mut prover, backend, proof_kind).await?;

    // ── Phase 14: Submit proof with permission output ────────────────────

    println!("\nPhase 14: Building and submitting proof...");
    let proof_tx_id = build_and_submit_proof(&node, &prove, proof_kind, &deploy, &keypair).await?;

    println!("  Waiting for proof tx confirmation...");
    wait_for_tx_confirmation(&node, proof_tx_id).await?;
    println!("  Proof tx confirmed!");

    // ── Phase 15: Withdraw #1 ────────────────────────────────────────────

    println!("\nPhase 15: Withdraw exit #1 ({exit1_amount} sompi)...");
    let delegate_addr = derive_delegate_address(deploy.on_chain_covenant_id, Prefix::Testnet);
    println!("  Delegate address: {delegate_addr}");

    let perm_redeem = prove.perm_redeem_script.as_ref().context("No permission redeem script — batch had no exits?")?;
    let w1 = withdraw_exit(
        &node,
        deploy.on_chain_covenant_id,
        (proof_tx_id, 1), // permission UTXO at proof tx output[1]
        deploy.output_value,
        perm_redeem,
        &prove.perm_exit_data,
        0, // leaf index 0 = exit #1
        &delegate_addr,
        &keypair,
    )
    .await?;

    println!("  Waiting for withdraw #1 confirmation...");
    wait_for_tx_confirmation(&node, w1.tx_id).await?;
    println!("  Withdraw #1 confirmed!");

    // ── Phase 16: Withdraw #2 ────────────────────────────────────────────

    println!("\nPhase 16: Withdraw exit #2 ({exit2_amount} sompi)...");
    let w1_cont = w1.continuation.context("Expected continuation permission after withdraw #1")?;

    let w2 = withdraw_exit(
        &node,
        deploy.on_chain_covenant_id,
        (w1.tx_id, 1), // continuation permission UTXO at withdraw #1 output[1]
        deploy.output_value,
        &w1_cont.perm_redeem,
        &w1_cont.exit_data,
        1, // leaf index 1 = exit #2 (leaf 0 already withdrawn)
        &delegate_addr,
        &keypair,
    )
    .await?;

    println!("  Waiting for withdraw #2 confirmation...");
    wait_for_tx_confirmation(&node, w2.tx_id).await?;
    println!("  Withdraw #2 confirmed!");

    // ── Summary ──────────────────────────────────────────────────────────

    println!("\n=== SUCCESS ===");
    println!("  Covenant ID:    {}", deploy.on_chain_covenant_id);
    println!("  Deploy tx:      {}", deploy.tx_id);
    println!("  Entry tx:       {entry_tx_id}");
    println!("  Exit #1 tx:     {exit1_tx_id}");
    println!("  Transfer tx:    {transfer_tx_id}");
    println!("  Exit #2 tx:     {exit2_tx_id}");
    println!("  Proof tx:       {proof_tx_id}");
    println!("  Withdraw #1 tx: {}", w1.tx_id);
    println!("  Withdraw #2 tx: {}", w2.tx_id);
    println!("  L2 operations:  entry({deposit_amount}), exit({exit1_amount}), transfer({transfer_amount}), exit({exit2_amount})");

    node.stop().await.context("Failed to stop node")?;
    Ok(())
}
