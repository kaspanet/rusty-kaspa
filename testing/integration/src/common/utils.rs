use super::client::ListeningClient;
use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_consensus_core::{
    constants::TX_VERSION,
    header::Header,
    sign::sign,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{
        MutableTransaction, ScriptPublicKey, SignableTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint,
        TransactionOutput, UtxoEntry,
    },
    utxo::{
        utxo_collection::{UtxoCollection, UtxoCollectionExtensions},
        utxo_diff::UtxoDiff,
    },
};
use kaspa_core::info;
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::{api::rpc::RpcApi, BlockAddedNotification, Notification, RpcUtxoEntry, VirtualDaaScoreChangedNotification};
use kaspa_txscript::pay_to_address_script;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use secp256k1::Keypair;
use std::{
    collections::{hash_map::Entry::Occupied, HashMap, HashSet},
    future::Future,
    sync::Arc,
    time::Duration,
};
use tokio::time::timeout;

pub(crate) const EXPAND_FACTOR: u64 = 1;
pub(crate) const CONTRACT_FACTOR: u64 = 1;

const fn estimated_mass(num_inputs: usize, num_outputs: u64) -> u64 {
    200 + 34 * num_outputs + 1000 * (num_inputs as u64)
}

pub const fn required_fee(num_inputs: usize, num_outputs: u64) -> u64 {
    const FEE_RATE: u64 = 10;
    FEE_RATE * estimated_mass(num_inputs, num_outputs)
}

/// Builds a TX DAG based on the initial UTXO set and on constant params
pub fn generate_tx_dag(
    mut utxoset: UtxoCollection,
    schnorr_key: Keypair,
    spk: ScriptPublicKey,
    target_levels: usize,
    target_width: usize,
) -> Vec<Arc<Transaction>> {
    /*
    Algo:
       perform level by level:
           for target txs per level:
               select random utxos (distinctly)
               create and sign a tx
               append tx to level txs
               append tx to utxo diff
           apply level utxo diff to the utxo collection
    */

    let num_inputs = CONTRACT_FACTOR as usize;
    let num_outputs = EXPAND_FACTOR;

    let mut txs = Vec::with_capacity(target_levels * target_width);

    for i in 0..target_levels {
        let mut utxo_diff = UtxoDiff::default();
        utxoset
            .iter()
            .take(num_inputs * target_width)
            .chunks(num_inputs)
            .into_iter()
            .map(|c| c.into_iter().map(|(o, e)| (TransactionInput::new(*o, vec![], 0, 1), e.clone())).unzip())
            .collect::<Vec<(Vec<_>, Vec<_>)>>()
            .into_par_iter()
            .map(|(inputs, entries)| {
                let total_in = entries.iter().map(|e| e.amount).sum::<u64>();
                let total_out = total_in - required_fee(num_inputs, num_outputs);
                let outputs = (0..num_outputs)
                    .map(|_| TransactionOutput { value: total_out / num_outputs, script_public_key: spk.clone() })
                    .collect_vec();
                let unsigned_tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
                sign(SignableTransaction::with_entries(unsigned_tx, entries), schnorr_key)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|signed_tx| {
                utxo_diff.add_transaction(&signed_tx.as_verifiable(), 0).unwrap();
                txs.push(Arc::new(signed_tx.tx));
            });
        utxoset.remove_collection(&utxo_diff.remove);
        utxoset.add_collection(&utxo_diff.add);

        if i % (target_levels / 10).max(1) == 0 {
            info!("Generated {} txs", txs.len());
        }
    }

    txs
}

/// Sanity test verifying that the generated TX DAG is valid, topologically ordered and has no double spends
pub fn verify_tx_dag(initial_utxoset: &UtxoCollection, txs: &[Arc<Transaction>]) {
    let mut prev_txs: HashMap<TransactionId, Arc<Transaction>> = HashMap::new();
    let mut used_outpoints = HashSet::with_capacity(txs.len() * 2);
    for tx in txs.iter() {
        for input in tx.inputs.iter() {
            assert!(used_outpoints.insert(input.previous_outpoint));
            if let Occupied(e) = prev_txs.entry(input.previous_outpoint.transaction_id) {
                assert!(e.get().outputs.len() > input.previous_outpoint.index as usize);
            } else {
                assert!(initial_utxoset.contains_key(&input.previous_outpoint));
            }
        }
        assert!(prev_txs.insert(tx.id(), tx.clone()).is_none());
    }
}

pub async fn wait_for<Fut>(sleep_millis: u64, max_iterations: u64, success: impl Fn() -> Fut, panic_message: &'static str)
where
    Fut: Future<Output = bool>,
{
    let mut i: u64 = 0;
    loop {
        i += 1;
        tokio::time::sleep(Duration::from_millis(sleep_millis)).await;
        if success().await {
            break;
        } else if i >= max_iterations {
            panic!("{}", panic_message);
        }
    }
}

pub fn generate_tx(
    schnorr_key: Keypair,
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    amount: u64,
    num_outputs: u64,
    address: &Address,
) -> Transaction {
    let total_in = utxos.iter().map(|x| x.1.amount).sum::<u64>();
    assert!(amount <= total_in - required_fee(utxos.len(), num_outputs));
    let script_public_key = pay_to_address_script(address);
    let inputs = utxos
        .iter()
        .map(|(op, _)| TransactionInput { previous_outpoint: *op, signature_script: vec![], sequence: 0, sig_op_count: 1 })
        .collect_vec();

    let outputs = (0..num_outputs)
        .map(|_| TransactionOutput { value: amount / num_outputs, script_public_key: script_public_key.clone() })
        .collect_vec();
    let unsigned_tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let signed_tx =
        sign(MutableTransaction::with_entries(unsigned_tx, utxos.iter().map(|(_, entry)| entry.clone()).collect_vec()), schnorr_key);
    signed_tx.tx
}

pub async fn fetch_spendable_utxos(
    client: &GrpcClient,
    address: Address,
    coinbase_maturity: u64,
) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let resp = client.get_utxos_by_addresses(vec![address.clone()]).await.unwrap();
    let virtual_daa_score = client.get_server_info().await.unwrap().virtual_daa_score;
    let mut utxos = Vec::with_capacity(resp.len());
    for resp_entry in
        resp.into_iter().filter(|resp_entry| is_utxo_spendable(&resp_entry.utxo_entry, virtual_daa_score, coinbase_maturity))
    {
        assert!(resp_entry.address.is_some());
        assert_eq!(*resp_entry.address.as_ref().unwrap(), address);
        utxos.push((TransactionOutpoint::from(resp_entry.outpoint), UtxoEntry::from(resp_entry.utxo_entry)));
    }
    utxos.sort_by(|a, b| b.1.amount.cmp(&a.1.amount));
    utxos
}

pub fn is_utxo_spendable(entry: &RpcUtxoEntry, virtual_daa_score: u64, coinbase_maturity: u64) -> bool {
    let needed_confirmations = if !entry.is_coinbase { 10 } else { coinbase_maturity };
    entry.block_daa_score + needed_confirmations <= virtual_daa_score
}

pub async fn mine_block(pay_address: Address, submitting_client: &GrpcClient, listening_clients: &[ListeningClient]) {
    // Discard all unreceived block added notifications in each listening client
    listening_clients.iter().for_each(|x| x.block_added_listener().unwrap().drain());

    // Mine a block
    let template = submitting_client.get_block_template(pay_address.clone(), vec![]).await.unwrap();
    let header: Header = (&template.block.header).into();
    let block_hash = header.hash;
    submitting_client.submit_block(template.block, false).await.unwrap();

    // Wait for each listening client to get notified the submitted block was added to the DAG
    for client in listening_clients.iter() {
        let block_daa_score: u64 = match timeout(Duration::from_millis(500), client.block_added_listener().unwrap().receiver.recv())
            .await
            .unwrap()
            .unwrap()
        {
            Notification::BlockAdded(BlockAddedNotification { block }) => {
                assert_eq!(block.header.hash, block_hash);
                block.header.daa_score
            }
            _ => panic!("wrong notification type"),
        };
        match timeout(Duration::from_millis(500), client.virtual_daa_score_changed_listener().unwrap().receiver.recv())
            .await
            .unwrap()
            .unwrap()
        {
            Notification::VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification { virtual_daa_score }) => {
                assert_eq!(virtual_daa_score, block_daa_score + 1);
            }
            _ => panic!("wrong notification type"),
        }
    }
}
