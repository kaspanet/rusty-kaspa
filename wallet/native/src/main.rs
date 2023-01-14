use kaspa_wallet_cli::kaspa_wallet_cli;

#[tokio::main]
async fn main() {
    let result = kaspa_wallet_cli().await;
    if let Err(err) = result {
        println!("{}", err);
    }
}
