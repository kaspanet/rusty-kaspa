use crate::{
    common::daemon::{ClientManager, Daemon},
    tasks::Task,
};
use async_trait::async_trait;
use clap::Parser;
use kaspa_addresses::Address;
use kaspa_consensus_core::network::NetworkType;
use kaspa_core::{trace, warn};
use kaspa_utils::{fd_budget, triggers::SingleTrigger};
use kaspad_lib::args::Args;
use std::{iter::once, sync::Arc};
use tokio::task::JoinHandle;

/// Arguments for configuring a [`DaemonTask`]
#[derive(Parser, Debug)]
pub struct DaemonArgs {
    /// Port of gRPC server
    #[arg(long)]
    pub rpc: u16,

    /// Port of P2P server
    #[arg(long)]
    pub p2p: u16,

    /// Preallocated UTXOs private key
    #[arg(long, name = "private-key")]
    pub private_key: String,

    #[arg(long, name = "stat-file-prefix")]
    pub stat_file_prefix: Option<String>,

    #[arg(long, name = "max-tracked-addresses")]
    pub max_tracked_addresses: usize,

    #[arg(long)]
    pub utxoindex: bool,
}

impl DaemonArgs {
    pub fn new(
        rpc: u16,
        p2p: u16,
        private_key: String,
        stat_file_prefix: Option<String>,
        max_tracked_addresses: usize,
        utxoindex: bool,
    ) -> Self {
        Self { rpc, p2p, private_key, stat_file_prefix, max_tracked_addresses, utxoindex }
    }

    pub fn from_env_args() -> Self {
        let mut collect_started: bool = false;
        let args = once("test".to_string()).chain(std::env::args().filter(|arg| {
            if *arg == "--" {
                collect_started = true;
                false
            } else {
                collect_started
            }
        }));
        DaemonArgs::parse_from(args)
    }

    pub fn to_command_args(&self, test_name: &str) -> Vec<String> {
        let mut args = vec![
            "test".to_owned(),
            "--package".to_owned(),
            "kaspa-testing-integration".to_owned(),
            "--lib".to_owned(),
            "--features".to_owned(),
            "devnet-prealloc".to_owned(),
            "--".to_owned(),
            test_name.to_owned(),
            "--exact".to_owned(),
            "--nocapture".to_owned(),
            "--ignored".to_owned(),
            "--".to_owned(),
            "--rpc".to_owned(),
            format!("{}", self.rpc),
            "--p2p".to_owned(),
            format!("{}", self.p2p),
            "--private-key".to_owned(),
            format!("{}", self.private_key),
            "--max-tracked-addresses".to_owned(),
            format!("{}", self.max_tracked_addresses),
        ];
        if let Some(ref stat_file_prefix) = self.stat_file_prefix {
            args.push("--stat-file-prefix".to_owned());
            args.push(stat_file_prefix.clone());
        }
        if self.utxoindex {
            args.push("--utxoindex".to_owned());
        }
        args
    }

    pub fn prealloc_address(&self) -> Address {
        let mut private_key_bytes = [0u8; 32];
        faster_hex::hex_decode(self.private_key.as_bytes(), &mut private_key_bytes).unwrap();
        let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key_bytes).unwrap();
        Address::new(
            NetworkType::Simnet.into(),
            kaspa_addresses::Version::PubKey,
            &schnorr_key.public_key().x_only_public_key().0.serialize(),
        )
    }

    #[cfg(feature = "devnet-prealloc")]
    pub fn apply_to(&self, args: &mut Args) {
        args.rpclisten = Some(format!("0.0.0.0:{}", self.rpc).try_into().unwrap());
        args.listen = Some(format!("0.0.0.0:{}", self.p2p).try_into().unwrap());
        args.prealloc_address = Some(self.prealloc_address().to_string());
        args.max_tracked_addresses = self.max_tracked_addresses;
        args.utxoindex = self.utxoindex;
    }

    #[cfg(not(feature = "devnet-prealloc"))]
    pub fn apply_to(&self, args: &mut Args) {
        args.rpclisten = Some(format!("0.0.0.0:{}", self.rpc).try_into().unwrap());
        args.listen = Some(format!("0.0.0.0:{}", self.p2p).try_into().unwrap());
        args.max_tracked_addresses = self.max_tracked_addresses;
        args.utxoindex = self.utxoindex;
    }
}

pub struct DaemonTask {
    client_manager: Arc<ClientManager>,
    ready_signal: SingleTrigger,
}

impl DaemonTask {
    pub fn build(client_manager: Arc<ClientManager>) -> Arc<Self> {
        Arc::new(Self { client_manager, ready_signal: SingleTrigger::new() })
    }

    pub fn with_args(args: Args) -> Arc<Self> {
        let client_manager = Arc::new(ClientManager::new(args));
        Self::build(client_manager)
    }

    async fn ready(&self) {
        self.ready_signal.listener.clone().await;
    }
}

#[async_trait]
impl Task for DaemonTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let ready_signal = self.ready_signal.trigger.clone();
        let fd_total_budget = fd_budget::limit();
        let mut daemon = Daemon::with_manager(self.client_manager.clone(), fd_total_budget);
        let task = tokio::spawn(async move {
            warn!("Daemon task starting...");
            daemon.run();

            // Wait for the node to initialize before connecting to RPC
            daemon.grpc_server_started().await;
            ready_signal.trigger();

            tokio::select! {
                biased;
                _ = daemon.shutdown_requested() => {
                    trace!("Daemon core shutdown was requested");
                    warn!("Daemon task signaling to stop");
                    stop_signal.trigger.trigger();
                }
                _ = stop_signal.listener.clone() => {
                    trace!("Daemon task got a stop signal");
                }
            }
            daemon.shutdown();
            warn!("Daemon task exited");
        });
        vec![task]
    }

    async fn ready(&self) {
        self.ready().await
    }
}
