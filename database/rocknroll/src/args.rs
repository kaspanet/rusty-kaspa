use std::path::PathBuf;

use clap::Args;
use kaspa_consensus_core::network::NetworkId;
use kaspad_lib::args::Args as KaspadArgs;

use crate::{Error, Result};

#[derive(Args, Debug, Clone)]
pub struct NetworkArgs {
    #[arg(long, help = "Use the test network")]
    pub testnet: bool,

    #[arg(long, default_value_t = 10, help = "Testnet network suffix number")]
    pub netsuffix: u32,

    #[arg(long, help = "Use the development test network")]
    pub devnet: bool,

    #[arg(long, help = "Use the simulation test network")]
    pub simnet: bool,
}

impl NetworkArgs {
    pub fn to_kaspad_args(&self, appdir: Option<String>) -> Result<KaspadArgs> {
        let selected = [self.testnet, self.devnet, self.simnet].into_iter().filter(|selected| *selected).count();
        if selected > 1 {
            return Err(Error::InvalidArgs("select at most one network flag: --testnet, --devnet, or --simnet".to_string()));
        }

        Ok(KaspadArgs {
            appdir,
            testnet: self.testnet,
            testnet_suffix: self.netsuffix,
            devnet: self.devnet,
            simnet: self.simnet,
            ..Default::default()
        })
    }

    pub fn network(&self) -> Result<NetworkId> {
        Ok(self.to_kaspad_args(None)?.network())
    }
}

#[derive(Args, Debug, Clone)]
pub struct DbSourceArgs {
    #[command(flatten)]
    pub network: NetworkArgs,

    #[arg(long, value_name = "APP_DIR", conflicts_with = "consensus_db", help = "Kaspad application directory")]
    pub appdir: Option<PathBuf>,

    #[arg(long, value_name = "CONSENSUS_DB", conflicts_with = "appdir", help = "Direct path to a consensus RocksDB directory")]
    pub consensus_db: Option<PathBuf>,

    #[arg(long, default_value_t = 128, help = "RocksDB max open files budget for the consensus DB")]
    pub files_limit: i32,
}
