//! `kaspa-rpc` binary entrypoint. Thin: parse args, run the engine, map errors
//! to a `sysexits`-style exit code.

use clap::Parser;
use kaspa_rpc_cli::{Cli, run};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    run(cli).await.unwrap_or_else(|err| {
        eprintln!("kaspa-rpc: {err}");
        ExitCode::from(err.exit_code())
    })
}
