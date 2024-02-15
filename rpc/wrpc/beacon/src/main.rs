mod args;
mod connection;
mod error;
pub mod imports;
mod log;
mod monitor;
mod node;
mod panic;
mod params;
mod result;
mod server;
mod transport;

use args::*;
use result::Result;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Error: {}", error);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = Args::parse();

    workflow_log::set_log_level(workflow_log::LevelFilter::Info);
    panic::init_ungraceful_panic_handler();

    println!();
    println!("Kaspa wRPC Beacon v{} starting...", env!("CARGO_PKG_VERSION"));

    monitor::init(&args);
    let (listener, app) = server::server(&args).await?;
    monitor::start().await?;
    axum::serve(listener, app).await?;
    monitor::stop().await?;
    Ok(())
}
