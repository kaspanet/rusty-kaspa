use kaspa_cli_lib::{kaspa_cli, TerminalOptions};

#[tokio::main]
async fn main() {
    let result = kaspa_cli(TerminalOptions::new().with_prompt("$ "), None).await;
    if let Err(err) = result {
        println!("{err}");
    }
}
