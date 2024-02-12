mod args;
mod error;
pub mod imports;
mod monitor;
mod node;
mod result;
mod server;

use args::*;
use monitor::monitor;
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
    let (listener, app) = server::server(&args).await?;
    monitor().start().await?;
    axum::serve(listener, app).await?;
    monitor().stop().await?;
    Ok(())
}
