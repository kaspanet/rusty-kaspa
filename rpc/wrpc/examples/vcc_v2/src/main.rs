// Example of VCCv2 endpoint

use kaspa_addresses::Address;
use kaspa_rpc_core::{api::rpc::RpcApi, RpcDataVerbosityLevel, RpcHash, RpcOptionalTransaction};
use kaspa_wrpc_client::{
    client::{ConnectOptions, ConnectStrategy},
    prelude::NetworkId,
    prelude::NetworkType,
    result::Result,
    KaspaRpcClient, WrpcEncoding,
};
use std::time::Duration;
use std::{collections::HashSet, process::ExitCode};

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

/// Helper to extract the sender address from the first input, if everything is present.
fn first_input_sender_address(tx: &RpcOptionalTransaction) -> Option<&Address> {
    let first_input = tx.inputs.first()?;

    let utxo_entry = first_input.verbose_data.as_ref()?.utxo_entry.as_ref()?;

    utxo_entry.verbose_data.as_ref()?.script_public_key_address.as_ref()
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

    let dag_info = client.get_block_dag_info().await?;
    // using pruning point as a demonstration, in a real usage you'd probably start from a checkpoint
    // and keep iterating checkpoints to checkpoints to get a live view (high reactivity environment)
    let pp_hash = dag_info.pruning_point_hash;

    let response = client.get_virtual_chain_from_block_v2(pp_hash, Some(RpcDataVerbosityLevel::High), None).await?;

    // keep track of accepted transaction ids
    let mut global_seen_tx = HashSet::<RpcHash>::with_capacity(30_000);

    for acd in response.chain_block_accepted_transactions.iter() {
        let header = acd.chain_block_header.clone();

        let Some(mergeset_block_hash) = header.hash else {
            eprintln!("chain_block_header.hash is missing");
            continue;
        };

        println!("mergeset of {} has {} accepted transactions", mergeset_block_hash, acd.accepted_transactions.len());

        for tx in &acd.accepted_transactions {
            let Some(verbose) = tx.verbose_data.as_ref() else {
                eprintln!("transaction.verbose_data is missing");
                continue;
            };

            let Some(id) = verbose.transaction_id else {
                eprintln!("transaction_id is missing in verbose_data");
                continue;
            };

            // Example: use transaction payload here (borrow instead of cloning)
            let _payload = &tx.payload;

            // Example: use first input's sender address, if available
            if let Some(sender_address) = first_input_sender_address(tx) {
                println!("{id} - {sender_address}");
            }

            global_seen_tx.insert(id);
        }
    }

    println!("total transactions count: {}", global_seen_tx.len());

    // Disconnect client from Kaspa node
    client.disconnect().await?;

    // Return function result
    Ok(())
}
