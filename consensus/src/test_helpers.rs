use std::fs::File;
use std::io::Write;

use kaspa_consensus_core::{
    BlockHashSet,
    block::Block,
    header::Header,
    subnets::SubnetworkId,
    tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_hashes::{HASH_SIZE, Hash};
use rand::{Rng, rngs::SmallRng, seq::SliceRandom};

pub fn header_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Header {
    Header::from_precomputed_hash(hash, parents)
}

pub fn block_from_precomputed_hash(hash: Hash, parents: Vec<Hash>) -> Block {
    Block::from_precomputed_hash(hash, parents)
}

pub fn generate_random_utxos_from_script_public_key_pool(
    rng: &mut SmallRng,
    amount: usize,
    script_public_key_pool: &[ScriptPublicKey],
) -> UtxoCollection {
    let mut i = 0;
    let mut collection = UtxoCollection::with_capacity(amount);
    while i < amount {
        collection
            .insert(generate_random_outpoint(rng), generate_random_utxo_from_script_public_key_pool(rng, script_public_key_pool));
        i += 1;
    }
    collection
}

pub fn generate_random_hash(rng: &mut SmallRng) -> Hash {
    let random_bytes = rng.r#gen::<[u8; HASH_SIZE]>();
    Hash::from_bytes(random_bytes)
}

pub fn generate_random_outpoint(rng: &mut SmallRng) -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash(rng), rng.r#gen::<u32>())
}

pub fn generate_random_utxo_from_script_public_key_pool(rng: &mut SmallRng, script_public_key_pool: &[ScriptPublicKey]) -> UtxoEntry {
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        script_public_key_pool.choose(rng).expect("expected_script_public key").clone(),
        rng.r#gen(),
        rng.gen_bool(0.5),
    )
}

pub fn generate_random_utxo(rng: &mut SmallRng) -> UtxoEntry {
    UtxoEntry::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        generate_random_p2pk_script_public_key(rng),
        rng.r#gen(),
        rng.gen_bool(0.5),
    )
}

///Note: this generates schnorr p2pk script public keys.
pub fn generate_random_p2pk_script_public_key(rng: &mut SmallRng) -> ScriptPublicKey {
    let mut script: ScriptVec = (0..32).map(|_| rng.r#gen()).collect();
    script.insert(0, 0x20);
    script.push(0xac);
    ScriptPublicKey::new(0_u16, script)
}

pub fn generate_random_hashes(rng: &mut SmallRng, amount: usize) -> Vec<Hash> {
    let mut hashes = Vec::with_capacity(amount);
    let mut i = 0;
    while i < amount {
        hashes.push(generate_random_hash(rng));
        i += 1;
    }
    hashes
}

///Note: generate_random_block is filled with random data, it does not represent a consensus-valid block!
pub fn generate_random_block(
    rng: &mut SmallRng,
    parent_amount: usize,
    number_of_transactions: usize,
    input_amount: usize,
    output_amount: usize,
) -> Block {
    Block::new(
        generate_random_header(rng, parent_amount),
        generate_random_transactions(rng, number_of_transactions, input_amount, output_amount),
    )
}

///Note: generate_random_header is filled with random data, it does not represent a consensus-valid header!
pub fn generate_random_header(rng: &mut SmallRng, parent_amount: usize) -> Header {
    Header::new_finalized(
        rng.r#gen(),
        vec![generate_random_hashes(rng, parent_amount)].try_into().unwrap(),
        generate_random_hash(rng),
        generate_random_hash(rng),
        generate_random_hash(rng),
        rng.r#gen(),
        rng.r#gen(),
        rng.r#gen(),
        rng.r#gen(),
        rng.r#gen::<u64>().into(),
        rng.r#gen(),
        generate_random_hash(rng),
    )
}

///Note: generate_random_transaction is filled with random data, it does not represent a consensus-valid transaction!
pub fn generate_random_transaction(rng: &mut SmallRng, input_amount: usize, output_amount: usize) -> Transaction {
    Transaction::new(
        rng.r#gen(),
        generate_random_transaction_inputs(rng, input_amount),
        generate_random_transaction_outputs(rng, output_amount),
        rng.r#gen(),
        SubnetworkId::from_byte(rng.r#gen()),
        rng.r#gen(),
        (0..20).map(|_| rng.r#gen::<u8>()).collect(),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transactions!
pub fn generate_random_transactions(rng: &mut SmallRng, amount: usize, input_amount: usize, output_amount: usize) -> Vec<Transaction> {
    Vec::from_iter((0..amount).map(move |_| generate_random_transaction(rng, input_amount, output_amount)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction input!
pub fn generate_random_transaction_input(rng: &mut SmallRng) -> TransactionInput {
    TransactionInput::new(
        generate_random_transaction_outpoint(rng),
        (0..32).map(|_| rng.r#gen::<u8>()).collect(),
        rng.r#gen(),
        rng.r#gen(),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_inputs(rng: &mut SmallRng, amount: usize) -> Vec<TransactionInput> {
    Vec::from_iter((0..amount).map(|_| generate_random_transaction_input(rng)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_output(rng: &mut SmallRng) -> TransactionOutput {
    TransactionOutput::new(
        rng.gen_range(1..100_000), //we choose small amounts as to not overflow with large utxosets.
        generate_random_p2pk_script_public_key(rng),
    )
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_outputs(rng: &mut SmallRng, amount: usize) -> Vec<TransactionOutput> {
    Vec::from_iter((0..amount).map(|_| generate_random_transaction_output(rng)))
}

///Note: generate_random_transactions is filled with random data, it does not represent consensus-valid  transaction output!
pub fn generate_random_transaction_outpoint(rng: &mut SmallRng) -> TransactionOutpoint {
    TransactionOutpoint::new(generate_random_hash(rng), rng.r#gen())
}

//TODO: create `assert_eq_<kaspa-struct>!()` helper macros in `consensus::test_helpers`
/// Utility to output a JSON representation of a DAG
pub fn dag_to_json(genesis: u64, blocks: &[(u64, Vec<u64>)]) -> serde_json::Value {
    let mut dag_data = serde_json::Map::new();
    dag_data.insert("genesis".to_string(), serde_json::Value::Number(genesis.into()));

    let blocks_array: Vec<serde_json::Value> = blocks
        .iter()
        .map(|(block, parents)| {
            let mut block_obj = serde_json::Map::new();
            block_obj.insert("id".to_string(), serde_json::Value::Number((*block).into()));
            block_obj.insert(
                "parents".to_string(),
                serde_json::Value::Array(parents.iter().map(|p| serde_json::Value::Number((*p).into())).collect()),
            );
            serde_json::Value::Object(block_obj)
        })
        .collect();

    dag_data.insert("blocks".to_string(), serde_json::Value::Array(blocks_array));
    serde_json::Value::Object(dag_data)
}

/// Utility to output a DOT/Graphviz representation of a DAG
pub fn dag_to_dot(genesis: u64, blocks: &Vec<(u64, Vec<u64>)>) -> String {
    let mut dot = String::from("digraph DAG {\n");
    dot.push_str(&format!("    {} [shape=doublecircle];\n", genesis));
    for (block, parents) in blocks {
        dot.push_str(&format!("    {} [shape=circle];\n", block));
        for parent in parents {
            dot.push_str(&format!("    {} -> {};\n", block, parent));
        }
    }
    dot.push_str("}\n");
    dot
}

pub fn generate_dot_with_chain(
    blocks: &[(u64, Vec<u64>)],
    chain_nodes: &BlockHashSet,
    reds: BlockHashSet,
    base_name: &str,
) -> std::io::Result<()> {
    let dot_filename = format!("{}.dot", base_name);
    let mut dot_file = File::create(&dot_filename)?;

    // Write DOT header
    writeln!(dot_file, "digraph DAG {{")?;
    writeln!(dot_file, "    // Graph settings")?;
    writeln!(dot_file, "    rankdir=TB;")?;
    writeln!(dot_file, "    node [fontname=\"Arial\", fontsize=10];")?;
    writeln!(dot_file, "    edge [fontname=\"Arial\", fontsize=8];")?;
    writeln!(dot_file)?;

    // Write node definitions
    for (block_id, _) in blocks {
        let block_hash = Hash::from_u64_word(*block_id);
        if chain_nodes.contains(&block_hash) {
            // Chain nodes get double circle
            writeln!(
                dot_file,
                "    {} [shape=doublecircle, color=blue, style=filled, fillcolor=lightsteelblue, penwidth=2];",
                block_id
            )?;
        } else if reds.contains(&block_hash) {
            // Non-chain nodes get regular circle
            writeln!(dot_file, "    {} [shape=circle, style=filled, fillcolor=lightcoral];", block_id)?;
        } else {
            // Non-chain nodes get regular circle
            writeln!(dot_file, "    {} [shape=circle, style=filled, fillcolor=lightskyblue];", block_id)?;
        }
    }

    writeln!(dot_file)?;

    // Write edge definitions
    for (block_id, parent_ids) in blocks {
        let from_node = Hash::from_u64_word(*block_id);

        if parent_ids.is_empty() {
            continue;
        }
        for &parent_id in parent_ids {
            let to_node = Hash::from_u64_word(parent_id);

            if chain_nodes.contains(&from_node) && chain_nodes.contains(&to_node) {
                // Chain edges get bold red
                writeln!(dot_file, "    {} -> {} [color=blue, penwidth=3, style=bold];", block_id, parent_id)?;
            } else {
                // Regular edges get gray dashed
                writeln!(dot_file, "    {} -> {} [color=gray, style=dashed];", block_id, parent_id)?;
            }
        }
    }

    writeln!(dot_file, "}}")?;

    println!("Generated DOT file: {}", dot_filename);
    Ok(())
}
