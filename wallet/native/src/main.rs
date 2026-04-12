use keryx_cli_lib::{TerminalOptions, keryx_cli};

#[tokio::main]
async fn main() {
    let result = keryx_cli(TerminalOptions::new().with_prompt("$ "), None).await;
    if let Err(err) = result {
        println!("{err}");
    }
}
