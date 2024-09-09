use std::time::Duration;
use kaspa_wrpc_client::{prelude::NetworkId, prelude::NetworkType, KaspaRpcClient, WrpcEncoding, result::Result, client::{ConnectOptions,ConnectStrategy}};
use kaspa_rpc_core::{api::rpc::RpcApi, GetServerInfoResponse, GetBlockDagInfoResponse};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match check_node_status().await {
        Ok(_) => {
            println!("Client connection correctly worked!");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("An error occurred: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn check_node_status() -> Result<()> {

    let encoding = WrpcEncoding::Borsh;
    let url = Some("ws://192.168.1.250:17110");
    let resolver = None;
    let network_id = Some(NetworkId::new(NetworkType::Mainnet));
    let subscription_context = None;
    
    let client = KaspaRpcClient::new(encoding, url, resolver, network_id, subscription_context)?;
    
    let timeout = 5_000;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(timeout)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };

    client.connect(Some(options)).await?;

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

    println!("");
    println!("Block count: {}", block_count);
    println!("Header count: {}", header_count);
    println!("Tip hash:");
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

    client.disconnect().await?;
    Ok(())

}

