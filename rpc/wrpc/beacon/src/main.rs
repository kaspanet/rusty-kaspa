mod args;
mod error;
pub mod imports;
mod monitor;
mod node;
mod result;
mod server;

use args::*;
// use tokio::*;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    print!("Hello, world! {:#?}", args);

    let (listener, app) = server::server(&args).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
