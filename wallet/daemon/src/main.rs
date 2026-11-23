mod args;

use crate::args::Args;
use kaspa_consensus_core::network::NetworkId;
use kaspa_core::{error, info, warn};
use kaspa_wallet_core::{
    api::WalletApi,
    rpc::{ConnectOptions, ConnectStrategy, Resolver, WrpcEncoding},
    utxo::set_user_transaction_maturity_period_daa,
    wallet::Wallet,
};
use kaspa_wallet_grpc_core::kaspawalletd::kaspawalletd_server::KaspawalletdServer;
use kaspa_wallet_grpc_server::{policy, service::Service};
use std::{error::Error, str::FromStr, sync::Arc, time::Duration};
use tokio::sync::oneshot;
use tonic::transport::Server;

const STARTUP_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const FORCED_EXIT_CODE: i32 = 130;

enum ShutdownTrigger {
    Request,
    Signal(&'static str),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    kaspa_core::log::init_logger(None, "");
    let args = Args::parse();
    let requested_network_id = args.network_id.as_deref().map(NetworkId::from_str).transpose()?;

    let wallet = Arc::new(Wallet::try_new(Wallet::local_store()?, Some(Resolver::default()), None)?);
    wallet.clone().wallet_open(args.password.into(), args.name, false, false).await?;
    info!("Wallet path: {}", wallet.store().location()?);

    if let Some(wrpc_client) = wallet.try_wrpc_client().as_ref() {
        let rpc_address = if let Some(address) = args.rpc_server {
            address
        } else {
            let network_id = requested_network_id.ok_or("either --rpc-server or --network-id must be specified")?;
            warn!("Using PNN may expose your data to third parties. For privacy, use a private, self-hosted node.");
            Resolver::default().get_url(WrpcEncoding::Borsh, network_id).await.map_err(|e| e.to_string())?
        };

        info!("Connecting to {}...", rpc_address);

        let options = ConnectOptions {
            block_async_connect: true,
            strategy: ConnectStrategy::Retry,
            url: Some(rpc_address.clone()),
            ..Default::default()
        };
        match tokio::time::timeout(STARTUP_CONNECT_TIMEOUT, wrpc_client.connect(Some(options))).await {
            Ok(result) => {
                result?;
            }
            Err(_) => {
                if let Err(err) = wrpc_client.disconnect().await {
                    error!("Failed to stop connecting to {}: {}", rpc_address, err);
                }
                return Err(
                    format!("failed to connect to {} within {} seconds", rpc_address, STARTUP_CONNECT_TIMEOUT.as_secs()).into()
                );
            }
        }
    }

    let dag_info = wallet.rpc_api().get_block_dag_info().await?;
    if let Some(network_id) = requested_network_id
        && network_id != dag_info.network
    {
        return Err(format!("--network-id {} does not match the node network {}", network_id, dag_info.network).into());
    }
    policy::ensure_supported_network(dag_info.network).map_err(|err| err.message().to_string())?;
    wallet.set_network_id(&dag_info.network)?;
    set_user_transaction_maturity_period_daa(&dag_info.network, 1);
    info!("Connected to node on {} with DAA score {}.", dag_info.network, dag_info.virtual_daa_score);

    wallet.start().await?;

    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let service = Service::with_notification_pipe_task(wallet.clone(), shutdown_sender, args.ecdsa);
    service.wallet().accounts_activate(None).await?;
    wallet.autoselect_default_account_if_single().await?;
    let account = wallet.account().map_err(|_| {
        format!("the daemon requires a wallet with exactly one account, found {} active accounts", wallet.active_accounts().len())
    })?;
    info!("Activated account {}, synchronizing...", account.id().short());

    let server_handle = tokio::spawn(async move {
        Server::builder()
            .add_service(KaspawalletdServer::new(service))
            .serve_with_shutdown(args.listen_address, async {
                match shutdown_trigger(shutdown_receiver).await {
                    ShutdownTrigger::Request => {
                        info!("Shutdown initiated by shutdown request, stopping gRPC server...");
                    }
                    ShutdownTrigger::Signal(signal) => {
                        info!("Shutdown initiated by {}, stopping gRPC server...", signal);
                        tokio::spawn(force_exit_on_next_signal());
                    }
                }
            })
            .await
    });
    info!("gRPC server is listening on {}:{}.", args.listen_address.ip(), args.listen_address.port());
    let server_result = server_handle.await;

    if let Err(err) = wallet.stop().await {
        error!("Failed to stop wallet: {}", err);
    }

    server_result??;

    Ok(())
}

async fn shutdown_trigger(shutdown_receiver: oneshot::Receiver<()>) -> ShutdownTrigger {
    tokio::select! {
        _ = shutdown_receiver => ShutdownTrigger::Request,
        signal = next_signal() => ShutdownTrigger::Signal(signal),
    }
}

async fn force_exit_on_next_signal() {
    let signal = next_signal().await;
    warn!("Received another {} during shutdown, exiting immediately", signal);
    std::process::exit(FORCED_EXIT_CODE);
}

async fn next_signal() -> &'static str {
    let interrupt = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            error!("Failed to listen for interrupt signal: {}", err);
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                error!("Failed to listen for SIGTERM: {}", err);
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => "interrupt signal",
        _ = terminate => "SIGTERM",
    }
}
