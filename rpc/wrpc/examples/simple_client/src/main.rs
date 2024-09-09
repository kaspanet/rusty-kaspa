//Example of simple client to connect with Kaspa node using wRPC connection and collect some node and network basic data 
//Verify your Kaspa node is runnning with --rpclisten-borsh=0.0.0.0:17110 parameter

use std::time::Duration;
use kaspa_wrpc_client::{prelude::NetworkId, prelude::NetworkType, KaspaRpcClient, WrpcEncoding, result::Result, client::{ConnectOptions,ConnectStrategy}};
use kaspa_rpc_core::{api::rpc::RpcApi, GetServerInfoResponse, GetBlockDagInfoResponse};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match check_node_status().await {
        Ok(_) => {
            println!("");
            println!("Well done! You successfully completed your first client connection to Kaspa node!");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("An error occurred: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn check_node_status() -> Result<()> {
    //Select encoding method to use, depending on your node settings
    let encoding = WrpcEncoding::Borsh;
    
    //Define your node address and wRPC port (see previous note)
    let url = Some("ws://0.0.0.0:17110");
    
    //Define the network your Kaspa node is connected to
    //You can select NetworkType::Mainnet, NetworkType::Testnet, NetworkType::Devnet, NetworkType::Simnet
    let network_type = NetworkType::Mainnet;
    let selected_network = Some(NetworkId::new(network_type));
    
    //Advanced options
    let resolver = None;
    let subscription_context = None;
    
    //Create new wRPC client with parameters defined above
    let client = KaspaRpcClient::new(encoding, url, resolver, selected_network, subscription_context)?;
    
    //Advanced connection options 
    let timeout = 5_000;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(timeout)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };

    //Connect to selected Kaspa node
    client.connect(Some(options)).await?;

    //Retrieve and show Kaspa node information
    let GetServerInfoResponse {
        is_synced,
        server_version,
        network_id,
        has_utxo_index,
        ..
    } = client.get_server_info().await?;

    println!("Node version: {}", server_version);
    println!("Network: {}", network_id);
    println!("Node is synced: {}", is_synced);
    println!("Node is indexing UTXOs: {}", has_utxo_index);
    println!("");

    //Retrieve and show Kaspa network information
    let GetBlockDagInfoResponse {
        block_count,
        header_count,
        tip_hashes,
        difficulty,
        past_median_time,
        virtual_parent_hashes,
        pruning_point_hash,
        virtual_daa_score,
        sink,
        ..
    } = client.get_block_dag_info().await?;

    println!("Block count: {}", block_count);
    println!("Header count: {}", header_count);
    println!("Tip hashes:");
    for tip_hash in tip_hashes{
        println!("{}", tip_hash);
    };
    println!("Difficulty: {}", difficulty);
    println!("Past median time: {}", past_median_time);
    println!("Virtual parent hashes:"); 
    for virtual_parent_hash in virtual_parent_hashes {
        println!("{}", virtual_parent_hash);
    }
    println!("Pruning point hash: {}", pruning_point_hash);
    println!("Virtual DAA score: {}", virtual_daa_score);
    println!("Sink: {}", sink);
    
    //Disconnect client from Kaspa node
    client.disconnect().await?;
    
    //Return function result
    Ok(())

}

