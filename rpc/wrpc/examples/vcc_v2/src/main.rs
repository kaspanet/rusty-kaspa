// Example of VCCv2 endpoint

use kaspa_rpc_core::{api::rpc::RpcApi, RpcDataVerbosityLevel, RpcHash};
use kaspa_wrpc_client::{
    client::{ConnectOptions, ConnectStrategy},
    prelude::NetworkId,
    prelude::NetworkType,
    result::Result,
    KaspaRpcClient, WrpcEncoding,
};
use std::str::FromStr;
use std::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    process::ExitCode,
};

#[tokio::main]
async fn main() -> ExitCode {
    match get_vcc_v2().await {
        Ok(_) => {
            println!("Well done! You successfully completed your first client connection to Kaspa node!");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("An error occurred: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn get_vcc_v2() -> Result<()> {
    let encoding = WrpcEncoding::Borsh;

    let url = Some("ws://127.0.0.1:17110");
    let resolver = None;

    let network_type = NetworkType::Mainnet;
    let selected_network = Some(NetworkId::new(network_type));

    // Advanced options
    let subscription_context = None;

    // Create new wRPC client with parameters defined above
    let client = KaspaRpcClient::new(encoding, url, resolver, selected_network, subscription_context)?;

    // Advanced connection options
    let timeout = 5_000;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(timeout)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };

    // Connect to selected Kaspa node
    client.connect(Some(options)).await?;

    let response = client
        .get_virtual_chain_from_block_v2(
            RpcHash::from_str("d274ca180b7ba85f512e4a59020c7a302a4eb9ba35331a3fd6e86c18bd3bac8c").unwrap(),
            Some(RpcDataVerbosityLevel::Low),
            None,
        )
        .await?;

    let mut global_seen_tx = HashSet::<RpcHash>::with_capacity(30_000);
    let mut tx_occurrence_counts = HashMap::<RpcHash, u64>::new();
    let mut total_duplicates = 0;
    let mut total_intra_mergeset_duplicates = 0;

    response.chain_block_accepted_transactions.iter().for_each(|acd| {
        let mut mergeset_seen_tx = HashSet::<RpcHash>::new();
        let mut current_mergeset_intra_duplicates = 0;
        let mergeset_block_hash = acd.chain_block_header.as_ref().hash.unwrap();

        acd.accepted_transactions.iter().for_each(|tx| {
            let id = tx.verbose_data.as_ref().unwrap().transaction_id.unwrap();

            *tx_occurrence_counts.entry(id).or_insert(0) += 1;

            if !global_seen_tx.insert(id) {
                total_duplicates += 1;
            }
            if !mergeset_seen_tx.insert(id) {
                current_mergeset_intra_duplicates += 1;
            }
        });

        println!("mergeset of {} has {} intra-duplicates", mergeset_block_hash, current_mergeset_intra_duplicates);
        total_intra_mergeset_duplicates += current_mergeset_intra_duplicates;
    });

    let total_cross_mergeset_duplicates = total_duplicates - total_intra_mergeset_duplicates;
    println!("total tx: {}", global_seen_tx.len());
    println!("total duplicated tx instances: {}", total_duplicates);
    println!("total intra-mergeset duplicated tx instances: {}", total_intra_mergeset_duplicates);
    println!("total cross-mergeset duplicated tx instances: {}", total_cross_mergeset_duplicates);

    println!("\nTransaction occurrence counts (duplicates only):");
    for (tx_id, count) in tx_occurrence_counts.iter() {
        if *count > 1 {
            println!("  Tx ID: {}, Occurrences: {}", tx_id, count);
        }
    }
    // Disconnect client from Kaspa node
    client.disconnect().await?;

    // Return function result
    Ok(())
}
