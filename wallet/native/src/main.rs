// Wallet code is exempt from the arithmetic-side-effects lint.
#![allow(clippy::arithmetic_side_effects)]

use kaspa_cli_lib::{TerminalOptions, kaspa_cli};

#[tokio::main]
async fn main() {
    let result = kaspa_cli(TerminalOptions::new().with_prompt("$ "), None).await;
    if let Err(err) = result {
        println!("{err}");
    }
}
