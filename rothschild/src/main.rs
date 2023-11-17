use std::{collections::HashMap, time::Duration};

use clap::{Arg, Command};
use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_consensus_core::{
    config::params::{TESTNET11_PARAMS, TESTNET_PARAMS},
    constants::TX_VERSION,
    sign::sign,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_core::{info, kaspad_env::version, time::unix_now, warn};
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::{api::rpc::RpcApi, notify::mode::NotificationMode};
use kaspa_txscript::pay_to_address_script;
use secp256k1::{rand::thread_rng, KeyPair};
use tokio::time::{interval, MissedTickBehavior};

const DEFAULT_SEND_AMOUNT: u64 = 10_000;

const FEE_PER_MASS: u64 = 10;

struct Stats {
    num_txs: usize,
    num_utxos: usize,
    utxos_amount: u64,
    num_outs: usize,
    since: u64,
}

pub struct Args {
    pub private_key: Option<String>,
    pub tps: u64,
    pub rpc_server: String,
}

impl Args {
    fn parse() -> Self {
        let m = cli().get_matches();
        Args {
            private_key: m.get_one::<String>("private-key").cloned(),
            tps: m.get_one::<u64>("tps").cloned().unwrap(),
            rpc_server: m.get_one::<String>("rpcserver").cloned().unwrap_or("localhost:16210".to_owned()),
        }
    }
}

pub fn cli() -> Command {
    Command::new("rothschild")
        .about(format!("{} (rothschild) v{}", env!("CARGO_PKG_DESCRIPTION"), version()))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::new("private-key").long("private-key").short('k').value_name("private-key").help("Private key in hex format"))
        .arg(
            Arg::new("tps")
                .long("tps")
                .short('t')
                .value_name("tps")
                .default_value("1")
                .value_parser(clap::value_parser!(u64))
                .help("Transactions per second"),
        )
        .arg(
            Arg::new("rpcserver")
                .long("rpcserver")
                .short('s')
                .value_name("rpcserver")
                .default_value("localhost:16210")
                .help("RPC server"),
        )
}

#[tokio::main]
async fn main() {
    kaspa_core::log::init_logger(None, "");
    let args = Args::parse();
    let mut stats = Stats { num_txs: 0, since: unix_now(), num_utxos: 0, utxos_amount: 0, num_outs: 0 };
    let rpc_client =
        GrpcClient::connect(NotificationMode::Direct, format!("grpc://{}", args.rpc_server), true, None, false, Some(500_000))
            .await
            .unwrap();
    info!("Connected to RPC");
    let mut pending = HashMap::new();

    let schnorr_key = if let Some(private_key_hex) = args.private_key {
        let mut private_key_bytes = [0u8; 32];
        faster_hex::hex_decode(private_key_hex.as_bytes(), &mut private_key_bytes).unwrap();
        secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).unwrap()
    } else {
        let (sk, pk) = &secp256k1::generate_keypair(&mut thread_rng());
        let kaspa_addr =
            Address::new(kaspa_addresses::Prefix::Testnet, kaspa_addresses::Version::PubKey, &pk.x_only_public_key().0.serialize());
        info!(
            "Generated private key {} and address {}. Send some funds to this address and rerun rothschild with `--private-key {}`",
            sk.display_secret(),
            String::from(&kaspa_addr),
            sk.display_secret()
        );
        return;
    };

    let kaspa_addr = Address::new(
        kaspa_addresses::Prefix::Testnet,
        kaspa_addresses::Version::PubKey,
        &schnorr_key.x_only_public_key().0.serialize(),
    );

    info!("Using Rothschild with private key {} and address {}", schnorr_key.display_secret(), String::from(&kaspa_addr));
    let info = rpc_client.get_block_dag_info().await.unwrap();
    let coinbase_maturity = match info.network.suffix {
        Some(11) => TESTNET11_PARAMS.coinbase_maturity,
        None | Some(_) => TESTNET_PARAMS.coinbase_maturity,
    };
    info!(
        "Node block-DAG info: \n\tNetwork: {}, \n\tBlock count: {}, \n\tHeader count: {}, \n\tDifficulty: {}, 
\tMedian time: {}, \n\tDAA score: {}, \n\tPruning point: {}, \n\tTips: {}, \n\t{} virtual parents: ...{}, \n\tCoinbase maturity: {}",
        info.network,
        info.block_count,
        info.header_count,
        info.difficulty,
        info.past_median_time,
        info.virtual_daa_score,
        info.pruning_point_hash,
        info.tip_hashes.len(),
        info.virtual_parent_hashes.len(),
        info.virtual_parent_hashes.last().unwrap(),
        coinbase_maturity,
    );

    let mut utxos = refresh_utxos(&rpc_client, kaspa_addr.clone(), &mut pending, coinbase_maturity).await;
    let mut ticker = interval(Duration::from_secs_f64(1.0 / (args.tps.min(100) as f64)));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut maximize_inputs = false;
    let mut last_refresh = unix_now();
    loop {
        ticker.tick().await;
        maximize_inputs = should_maximize_inputs(maximize_inputs, &utxos, &pending);
        let now = unix_now();
        let has_funds =
            maybe_send_tx(&rpc_client, kaspa_addr.clone(), &mut utxos, &mut pending, schnorr_key, &mut stats, maximize_inputs).await;
        if !has_funds {
            info!("Has not enough funds");
        }
        if !has_funds || now - last_refresh > 60_000 {
            info!("Refetching UTXO set");
            tokio::time::sleep(Duration::from_millis(100)).await; // We don't want this operation to be too frequent since its heavy on the node, so we wait some time before executing it.
            utxos = refresh_utxos(&rpc_client, kaspa_addr.clone(), &mut pending, coinbase_maturity).await;
            last_refresh = unix_now();
            pause_if_mempool_is_full(&rpc_client).await;
        }
        clean_old_pending_outpoints(&mut pending);
    }
}

fn should_maximize_inputs(
    old_value: bool,
    utxos: &Vec<(TransactionOutpoint, UtxoEntry)>,
    pending: &HashMap<TransactionOutpoint, u64>,
) -> bool {
    let estimated_utxos = if utxos.len() > pending.len() { utxos.len() - pending.len() } else { 0 };
    if !old_value && estimated_utxos > 1_000_000 {
        info!("Starting to maximize inputs");
        true
    } else if old_value && estimated_utxos < 500_000 {
        info!("Stopping to maximize inputs");
        false
    } else {
        old_value
    }
}

async fn pause_if_mempool_is_full(rpc_client: &GrpcClient) {
    loop {
        let mempool_size = rpc_client.get_info().await.unwrap().mempool_size;
        if mempool_size < 10_000 {
            break;
        }

        const PAUSE_DURATION: u64 = 10;
        info!("Mempool has {} entries. Pausing for {} seconds to reduce mempool pressure", mempool_size, PAUSE_DURATION);
        tokio::time::sleep(Duration::from_secs(PAUSE_DURATION)).await;
    }
}

async fn refresh_utxos(
    rpc_client: &GrpcClient,
    kaspa_addr: Address,
    pending: &mut HashMap<TransactionOutpoint, u64>,
    coinbase_maturity: u64,
) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    populate_pending_outpoints_from_mempool(rpc_client, kaspa_addr.clone(), pending).await;
    fetch_spendable_utxos(rpc_client, kaspa_addr, coinbase_maturity).await
}

async fn populate_pending_outpoints_from_mempool(
    rpc_client: &GrpcClient,
    kaspa_addr: Address,
    pending_outpoints: &mut HashMap<TransactionOutpoint, u64>,
) {
    let entries = rpc_client.get_mempool_entries_by_addresses(vec![kaspa_addr], true, false).await.unwrap();
    let now = unix_now();
    for entry in entries {
        for entry in entry.sending {
            for input in entry.transaction.inputs {
                pending_outpoints.insert(input.previous_outpoint, now);
            }
        }
    }
}

async fn fetch_spendable_utxos(
    rpc_client: &GrpcClient,
    kaspa_addr: Address,
    coinbase_maturity: u64,
) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let resp = rpc_client.get_utxos_by_addresses(vec![kaspa_addr]).await.unwrap();
    let dag_info = rpc_client.get_block_dag_info().await.unwrap();
    let mut utxos = Vec::with_capacity(resp.len());
    for resp_entry in
        resp.into_iter().filter(|resp_entry| is_utxo_spendable(&resp_entry.utxo_entry, dag_info.virtual_daa_score, coinbase_maturity))
    {
        utxos.push((resp_entry.outpoint, resp_entry.utxo_entry));
    }
    utxos.sort_by(|a, b| b.1.amount.cmp(&a.1.amount));
    utxos
}

fn is_utxo_spendable(entry: &UtxoEntry, virtual_daa_score: u64, coinbase_maturity: u64) -> bool {
    let needed_confs = if !entry.is_coinbase {
        10
    } else {
        coinbase_maturity * 2 // TODO: We should compare with sink blue score in the case of coinbase
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
    maximize_inputs: bool,
) -> bool {
    let num_outs = if maximize_inputs { 1 } else { 2 };
    let (selected_utxos, selected_amount) = select_utxos(utxos, DEFAULT_SEND_AMOUNT, num_outs, maximize_inputs, pending);
    if selected_amount == 0 {
        return false;
    }

    let tx = generate_tx(schnorr_key, &selected_utxos, selected_amount, num_outs, &kaspa_addr);

    let now = unix_now();
    for input in tx.inputs.iter() {
        pending.insert(input.previous_outpoint, now);
    }

    match rpc_client.submit_transaction((&tx).into(), false).await {
        Ok(_) => {}
        Err(e) => {
            warn!("RPC error: {}", e);
            return true;
        }
    }

    stats.num_txs += 1;
    stats.num_utxos += selected_utxos.len();
    stats.utxos_amount += selected_utxos.into_iter().map(|(_, entry)| entry.amount).sum::<u64>();
    stats.num_outs += tx.outputs.len();
    let now = unix_now();
    let time_past = now - stats.since;
    if time_past > 10_000 {
        info!(
            "Tx rate: {:.1}/sec, avg UTXO amount: {}, avg UTXOs per tx: {}, avg outs per tx: {}, estimated available UTXOs: {}",
            1000f64 * (stats.num_txs as f64) / (time_past as f64),
            (stats.utxos_amount / stats.num_utxos as u64),
            stats.num_utxos / stats.num_txs,
            stats.num_outs / stats.num_txs,
            if utxos.len() > pending.len() { utxos.len() - pending.len() } else { 0 },
        );
        stats.since = now;
        stats.num_txs = 0;
        stats.num_utxos = 0;
        stats.utxos_amount = 0;
        stats.num_outs = 0;
    }

    true
}

fn clean_old_pending_outpoints(pending: &mut HashMap<TransactionOutpoint, u64>) {
    let now = unix_now();
    let old_keys = pending.iter().filter(|(_, time)| now - *time > 3600 * 1000).map(|(op, _)| *op).collect_vec();
    for key in old_keys {
        pending.remove(&key).unwrap();
    }
}

fn required_fee(num_utxos: usize, num_outs: u64) -> u64 {
    FEE_PER_MASS * estimated_mass(num_utxos, num_outs)
}

fn estimated_mass(num_utxos: usize, num_outs: u64) -> u64 {
    200 + 34 * num_outs + 1000 * (num_utxos as u64)
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
        .map(|_| TransactionOutput { value: send_amount / num_outs, script_public_key: script_public_key.clone() })
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
    maximize_utxos: bool,
    pending: &HashMap<TransactionOutpoint, u64>,
) -> (Vec<(TransactionOutpoint, UtxoEntry)>, u64) {
    const MAX_UTXOS: usize = 84;
    let mut selected_amount: u64 = 0;
    let mut selected = Vec::new();
    for (outpoint, entry) in utxos.iter().filter(|(op, _)| !pending.contains_key(op)).cloned() {
        selected_amount += entry.amount;
        selected.push((outpoint, entry));

        let fee = required_fee(selected.len(), num_outs);

        if selected_amount >= min_amount + fee && (!maximize_utxos || selected.len() == MAX_UTXOS) {
            return (selected, selected_amount - fee);
        }

        if selected.len() > MAX_UTXOS {
            return (vec![], 0);
        }
    }

    (vec![], 0)
}
