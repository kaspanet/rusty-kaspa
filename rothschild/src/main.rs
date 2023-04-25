use std::{collections::HashMap, time::Duration};

use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_consensus_core::{
    constants::{SOMPI_PER_KASPA, TX_VERSION},
    sign::sign,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_core::{info, time::unix_now, warn};
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::{api::rpc::RpcApi, RpcTransaction, RpcTransactionInput, RpcTransactionOutput};
use kaspa_txscript::pay_to_address_script;
use secp256k1::KeyPair;
use tokio::time::{interval, MissedTickBehavior};

// TODO: pass in CLI args.
const PRIVATE_KEY: &'static str = "3674b992068d6aa3ca47303d423df9be48a5b147da4884517ce8468a4b2c80a0";
const ADDRESS: &'static str = "kaspatest:qr0est6tpap7my72c9fw4layj4uhfuvxf4vgh3ypj5xwmcqqvr28zyp62x7kp";
const TX_INTERVAL: u64 = 3; // In millis.
const DEFAULT_SEND_AMOUNT: u64 = 10_000;
const FEE_PER_MASS: u64 = 10;
const BIG_TX_NUM_OUTS: u64 = 50;

struct Stats {
    num_txs: usize,
    num_utxos: usize,
    utxos_amount: u64,
    num_outs: usize,
    since: u64,
}

#[tokio::main]
async fn main() {
    kaspa_core::log::init_logger("");
    // TODO: Pass address in CLI args
    let mut stats = Stats { num_txs: 0, since: unix_now(), num_utxos: 0, utxos_amount: 0, num_outs: 0 };
    let rpc_client = GrpcClient::connect("grpc://104.11.218.90:16210".into(), true, None, false, Some(50_000)).await.unwrap();
    let kaspa_addr = Address::try_from(ADDRESS).unwrap();
    let mut pending = HashMap::new();
    let mut utxos = refresh_utxos(&rpc_client, kaspa_addr.clone(), &mut pending).await;

    let mut refill_mode = false;
    let mut private_key_bytes = [0u8; 32];
    faster_hex::hex_decode(PRIVATE_KEY.as_bytes(), &mut private_key_bytes).unwrap();
    let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).unwrap();
    let mut ticker = interval(Duration::from_millis(TX_INTERVAL));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Burst);

    loop {
        ticker.tick().await;
        if !maybe_send_tx(&rpc_client, kaspa_addr.clone(), &mut utxos, &mut pending, schnorr_key, &mut stats, &mut refill_mode).await {
            info!("Has not enough funds. Refetchin UTXO set");
            tokio::time::sleep(Duration::from_millis(100)).await;
            utxos = refresh_utxos(&rpc_client, kaspa_addr.clone(), &mut pending).await;
        }
    }
}

async fn refresh_utxos(
    rpc_client: &GrpcClient,
    kaspa_addr: Address,
    pending: &mut HashMap<TransactionOutpoint, u64>,
) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    populate_pending_outpoints_from_mempool(&rpc_client, kaspa_addr.clone(), pending).await;
    fetch_spendable_utxos(&rpc_client, kaspa_addr).await
}

async fn populate_pending_outpoints_from_mempool(
    rpc_client: &GrpcClient,
    kaspa_addr: Address,
    pending_outpoints: &mut HashMap<TransactionOutpoint, u64>,
) {
    // info!("Populating pending transactions from the mempool");
    let entries = rpc_client.get_mempool_entries_by_addresses(vec![kaspa_addr], false, false).await.unwrap();
    let now = unix_now();
    for entry in entries {
        for entry in entry.sending {
            for input in entry.transaction.inputs {
                pending_outpoints.insert(input.previous_outpoint, now);
            }
        }
    }
}

async fn fetch_spendable_utxos(rpc_client: &GrpcClient, kaspa_addr: Address) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let resp = rpc_client.get_utxos_by_addresses(vec![kaspa_addr]).await.unwrap();
    let dag_info = rpc_client.get_block_dag_info().await.unwrap();
    let mut utxos = Vec::with_capacity(resp.len());
    for resp_entry in resp.into_iter().filter(|resp_entry| is_utxo_spendable(&resp_entry.utxo_entry, dag_info.virtual_daa_score)) {
        utxos.push((resp_entry.outpoint, resp_entry.utxo_entry));
    }
    utxos.sort_by(|a, b| b.1.amount.cmp(&a.1.amount));
    utxos
}

fn is_utxo_spendable(entry: &UtxoEntry, virtual_daa_score: u64) -> bool {
    let needed_confs = if !entry.is_coinbase {
        10
    } else {
        const COINBASE_MATURITY: u64 = 100; // TODO: Take from Params.
        COINBASE_MATURITY * 2 // TODO: We should compare with sink blue score in the case of coinbase
    };
    entry.block_daa_score + needed_confs < virtual_daa_score
}

async fn maybe_send_tx(
    rpc_client: &GrpcClient,
    kaspa_addr: Address,
    utxos: &mut Vec<(TransactionOutpoint, UtxoEntry)>,
    pending: &mut HashMap<TransactionOutpoint, u64>,
    schnorr_key: KeyPair,
    stats: &mut Stats,
    refill_mode: &mut bool,
) -> bool {
    let mut send_amount = DEFAULT_SEND_AMOUNT;
    let mut num_outs = 2;
    let mut max_utxos = 84;
    // if utxos.len() < 1000 || (*refill_mode && utxos.len() < 100_000) {
    //     if !*refill_mode {
    //         *refill_mode = true;
    //         info!("Entering refill mode");
    //     }
    //     send_amount *= BIG_TX_NUM_OUTS / num_outs;
    //     num_outs = BIG_TX_NUM_OUTS;
    //     max_utxos = 25;
    // } else if *refill_mode {
    //     info!("Exiting refill mode");
    //     *refill_mode = false;
    // }

    let (selected_utxos, selected_amount) = select_utxos(utxos, send_amount, num_outs, max_utxos, pending);
    if selected_amount == 0 {
        return false;
    }

    let tx = generate_tx(schnorr_key, &selected_utxos, selected_amount, num_outs, &kaspa_addr);

    let now = unix_now();
    for input in tx.inputs.iter() {
        pending.insert(input.previous_outpoint, now);
    }

    match rpc_client.submit_transaction(tx_to_rpc_tx(&tx), false).await {
        Ok(_) => {}
        Err(e) => {
            warn!("RPC error: {}", e);
            return true;
        }
    }
    // info!("Sent transaction {} worth {} kaspa with {} inputs and {} outputs", tx.id(), send_amount, tx.inputs.len(), tx.outputs.len());

    stats.num_txs += 1;
    stats.num_utxos += selected_utxos.len();
    stats.utxos_amount += selected_utxos.into_iter().map(|(_, entry)| entry.amount).sum::<u64>();
    stats.num_outs += tx.outputs.len();
    let now = unix_now();
    let time_past = now - stats.since;
    if time_past > 10_000 {
        info!(
            "Tx rate: {}/sec, avg UTXO amount: {}, avg UTXOs per tx: {}, avg outs per tx: {}, estimated available UTXOs: {}",
            1000f64 * (stats.num_txs as f64) / (time_past as f64),
            (stats.utxos_amount / stats.num_utxos as u64),
            stats.num_utxos / stats.num_txs,
            stats.num_outs / stats.num_txs,
            utxos.len(),
        );
        stats.since = now;
        stats.num_txs = 0;
        stats.num_utxos = 0;
        stats.utxos_amount = 0;
        stats.num_outs = 0;
    }

    true
}

fn required_fee(num_utxos: usize, num_outs: u64) -> u64 {
    FEE_PER_MASS * estimated_mass(num_utxos, num_outs)
}

fn estimated_mass(num_utxos: usize, num_outs: u64) -> u64 {
    200 + 34 * num_outs + 1000 * (num_utxos as u64)
}

fn tx_to_rpc_tx(tx: &Transaction) -> RpcTransaction {
    RpcTransaction {
        version: tx.version,
        inputs: tx
            .inputs
            .iter()
            .map(|input| RpcTransactionInput {
                previous_outpoint: input.previous_outpoint,
                signature_script: input.signature_script.clone(),
                sequence: input.sequence,
                sig_op_count: input.sig_op_count,
                verbose_data: None,
            })
            .collect_vec(),
        outputs: tx
            .outputs
            .iter()
            .map(|out| RpcTransactionOutput { value: out.value, script_public_key: out.script_public_key.clone(), verbose_data: None })
            .collect_vec(),
        lock_time: tx.lock_time,
        subnetwork_id: tx.subnetwork_id.clone(),
        gas: tx.gas,
        payload: tx.payload.clone(),
        verbose_data: None,
    }
}

fn generate_tx(
    schnorr_key: KeyPair,
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    send_amount: u64,
    num_outs: u64,
    kaspa_addr: &Address,
) -> Transaction {
    let script_public_key = pay_to_address_script(kaspa_addr);
    let inputs = utxos
        .iter()
        .map(|(op, _)| TransactionInput { previous_outpoint: *op, signature_script: vec![], sequence: 0, sig_op_count: 1 })
        .collect_vec();

    let outputs = (0..num_outs)
        .map(|_| TransactionOutput { value: send_amount / (num_outs as u64), script_public_key: script_public_key.clone() })
        .collect_vec();
    let unsigned_tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signed_tx =
        sign(MutableTransaction::with_entries(unsigned_tx, utxos.iter().map(|(_, entry)| entry.clone()).collect_vec()), schnorr_key);
    signed_tx.tx
}

fn select_utxos(
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    min_amount: u64,
    num_outs: u64,
    max_utxos: usize,
    pending: &HashMap<TransactionOutpoint, u64>,
) -> (Vec<(TransactionOutpoint, UtxoEntry)>, u64) {
    let mut selected_amount: u64 = 0;
    let mut selected = Vec::new();
    for (outpoint, entry) in utxos.iter().cloned().filter(|(op, _)| !pending.contains_key(op)) {
        selected_amount += entry.amount;
        selected.push((outpoint, entry));

        let fee = required_fee(selected.len(), num_outs);

        if selected_amount >= min_amount + fee {
            return (selected, selected_amount - fee);
        }

        if selected.len() == max_utxos {
            return (vec![], 0);
        }
    }

    (vec![], 0)
}
