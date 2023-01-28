use kaspa_wallet_cli::{kaspa_wallet_cli, TerminalOptions};

#[tokio::main]
async fn main() {
    let result = kaspa_wallet_cli(TerminalOptions::new().with_prompt("$ ")).await;
    if let Err(err) = result {
        println!("{err}");
    }
}
