//! Shared simnet block and signed-transaction assertions for daemon and NAT tests.
use super::utils::fetch_spendable_utxos;
use kaspa_addresses::Address;
use kaspa_consensus_core::{
    config::params::SIMNET_PARAMS,
    constants::TX_VERSION_TOCCATA,
    header::Header,
    mass::ComputeBudget,
    sign::sign_with_multiple_v2,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutput},
};
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_txscript::pay_to_address_script;
use std::time::Duration;
async fn wait(mut predicate: impl AsyncFnMut() -> bool, timeout: Duration) {
    tokio::time::timeout(timeout, async {
        while !predicate().await {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Libp2p synchronization condition timed out");
}

async fn mine(client: &GrpcClient, address: &Address, timeout: Duration) {
    let template = client.get_block_template(address.clone(), vec![]).await.unwrap();
    let header: Header = (&template.block.header).try_into().unwrap();
    client.submit_block(template.block, false).await.unwrap();
    wait(async || client.get_block_dag_info().await.unwrap().sink == header.hash, timeout).await;
}

pub async fn exchange(a: &GrpcClient, b: &GrpcClient, timeout: Duration) {
    let (secret, public) = secp256k1::generate_keypair(&mut rand::thread_rng());
    let miner =
        Address::new(kaspa_addresses::Prefix::Simnet, kaspa_addresses::Version::PubKey, &public.x_only_public_key().0.serialize());
    for _ in 0..SIMNET_PARAMS.coinbase_maturity() + 12 {
        mine(a, &miner, timeout).await;
    }
    let sink = a.get_block_dag_info().await.unwrap().sink;
    wait(async || b.get_block_dag_info().await.unwrap().sink == sink, timeout).await;
    let utxos = fetch_spendable_utxos(a, miner.clone(), SIMNET_PARAMS.coinbase_maturity()).await;
    let (outpoint, entry) = utxos.first().expect("mature coinbase UTXO");
    let unsigned = Transaction::new(
        TX_VERSION_TOCCATA,
        vec![TransactionInput {
            previous_outpoint: *outpoint,
            signature_script: vec![],
            sequence: 0,
            compute_commit: ComputeBudget(0).into(),
        }],
        vec![TransactionOutput { value: entry.amount - 10_000_000, script_public_key: pay_to_address_script(&miner), covenant: None }],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    let mut transaction =
        sign_with_multiple_v2(MutableTransaction::with_entries(unsigned, vec![entry.clone()]), &[secret.secret_bytes()]).unwrap().tx;
    transaction.inputs[0].compute_commit = ComputeBudget(300).into();
    transaction.finalize();
    // Kaspa batches transaction announcements for 500ms after the last block flush.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let transaction_id = a.submit_transaction((&transaction).into(), false).await.unwrap();
    wait(async || b.get_mempool_entry(transaction_id, false, false).await.is_ok(), timeout).await;
    mine(a, &miner, timeout).await;
    let sink = a.get_block_dag_info().await.unwrap().sink;
    wait(async || b.get_block_dag_info().await.unwrap().sink == sink, timeout).await;
}
