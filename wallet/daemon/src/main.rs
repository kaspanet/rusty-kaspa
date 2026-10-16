mod args;

use crate::args::Args;
use kaspa_consensus_core::network::NetworkId;
use kaspa_core::{info, warn};
use kaspa_wallet_core::{
    api::WalletApi,
    rpc::{ConnectOptions, ConnectStrategy, Resolver, WrpcEncoding},
    wallet::Wallet,
};
use kaspa_wallet_grpc_core::kaspawalletd::kaspawalletd_server::KaspawalletdServer;
use kaspa_wallet_grpc_server::service::Service;
use std::{error::Error, str::FromStr, sync::Arc};
use tokio::sync::oneshot;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    kaspa_core::log::init_logger(None, "");
    let args = Args::parse();

    let wallet = Arc::new(Wallet::try_new(Wallet::local_store()?, Some(Resolver::default()), None)?);
    wallet.clone().wallet_open(args.password.into(), args.name, false, false).await?;
    info!("Wallet path: {}", wallet.store().location()?);

    if let Some(wrpc_client) = wallet.try_wrpc_client().as_ref() {
        let rpc_address = if let Some(address) = args.rpc_server {
            address
        } else {
            let network_id = NetworkId::from_str(&args.network_id.expect("Specifying network id is needed for PNN"))?;
            warn!("Using PNN may expose your data to third parties. For privacy, use a private, self-hosted node.");
            Resolver::default().get_url(WrpcEncoding::Borsh, network_id).await.map_err(|e| e.to_string())?
        };

        info!("Connecting to {}...", rpc_address);

        let options = ConnectOptions {
            block_async_connect: true,
            strategy: ConnectStrategy::Fallback,
            url: Some(rpc_address),
            ..Default::default()
        };
        wrpc_client.connect(Some(options)).await?;
    }

    let dag_info = wallet.rpc_api().get_block_dag_info().await?;
    wallet.set_network_id(&dag_info.network)?;
    info!("Connected to node on {} with DAA score {}.", dag_info.network, dag_info.virtual_daa_score);

    wallet.start().await?;

    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let service = Service::with_notification_pipe_task(wallet.clone(), shutdown_sender, args.ecdsa);
    service.wallet().accounts_activate(None).await?;
    wallet.autoselect_default_account_if_single().await?;
    info!("Activated account {}, synchronizing...", wallet.account().unwrap().id().short());

    let server_handle = tokio::spawn(async move {
        Server::builder()
            .add_service(KaspawalletdServer::new(service))
            .serve_with_shutdown(args.listen_address, async {
                let _ = shutdown_receiver.await;
                info!("Shutdown initiated, stopping gRPC server...");
            })
            .await
            .unwrap();
    });
    info!("gRPC server is listening on {}:{}.", args.listen_address.ip(), args.listen_address.port());
    server_handle.await?;

    Ok(())
}
