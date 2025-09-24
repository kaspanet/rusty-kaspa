// Example of VCCv2 endpoint

use kaspa_rpc_core::{
    api::rpc::RpcApi, RpcAcceptanceDataVerbosity, RpcHash, RpcHeaderVerbosity, RpcMergesetBlockAcceptanceDataVerbosity,
    RpcTransactionInputVerboseDataVerbosity, RpcTransactionInputVerbosity, RpcTransactionOutputVerbosity,
    RpcTransactionVerboseDataVerbosity, RpcTransactionVerbosity, RpcUtxoEntryVerbosity,
};
use kaspa_wrpc_client::{
    client::{ConnectOptions, ConnectStrategy},
    prelude::NetworkId,
    prelude::NetworkType,
    result::Result,
    KaspaRpcClient, WrpcEncoding,
};
use std::str::FromStr;
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
        .get_virtual_chain_from_block_v_2(
            RpcHash::from_str("cc3ba81f23d6fb0d0ef785100815c64609d9bdf8e442b34f71e392fbe86d47a7").unwrap(),
            Some(RpcAcceptanceDataVerbosity::new(
                Some(RpcHeaderVerbosity { include_hash: Some(true), ..Default::default() }),
                Some(RpcMergesetBlockAcceptanceDataVerbosity {
                    merged_header_verbosity: Some(RpcHeaderVerbosity { include_hash: Some(true), ..Default::default() }),
                    accepted_transactions_verbosity: Some(RpcTransactionVerbosity {
                        input_verbosity: Some(RpcTransactionInputVerbosity {
                            include_signature_script: Some(true),
                            include_previous_outpoint: Some(true),
                            verbose_data_verbosity: Some(RpcTransactionInputVerboseDataVerbosity {
                                utxo_entry_verbosity: Some(RpcUtxoEntryVerbosity { include_amount: Some(true), ..Default::default() }),
                            }),
                            ..Default::default()
                        }),
                        output_verbosity: Some(RpcTransactionOutputVerbosity {
                            include_script_public_key: Some(true),
                            include_amount: Some(true),
                            ..Default::default()
                        }),
                        verbose_data_verbosity: Some(RpcTransactionVerboseDataVerbosity {
                            include_transaction_id: Some(true),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                }),
            )),
        )
        .await?;

    let mut count_mergeset = 0;
    let mut count_block = 0;
    let mut seen_tx = HashSet::<RpcHash>::with_capacity(30_000);
    let mut count_duplicated_tx = 0;
    response.added_acceptance_data.iter().for_each(|acd| {
        // println!("mergeset of {}", acd.accepting_chain_header.as_ref().unwrap().hash.unwrap());

        count_mergeset += 1;
        acd.mergeset_block_acceptance_data.iter().for_each(|bad| {
            // println!("block {}", bad.merged_header.as_ref().unwrap().hash.unwrap());
            count_block += 1;

            bad.accepted_transactions.iter().for_each(|tx| {
                let id = tx.verbose_data.as_ref().unwrap().transaction_id.unwrap();
                if seen_tx.contains(&id) {
                    count_duplicated_tx += 1;
                }
                seen_tx.insert(id);
            });
        })
    });

    println!("ms: {}, b: {}", count_mergeset, count_block);
    println!("duplicated tx {}", count_duplicated_tx);
    // Disconnect client from Kaspa node
    client.disconnect().await?;

    // Return function result
    Ok(())
}
