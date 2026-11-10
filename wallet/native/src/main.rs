use aero_cli_lib::{TerminalOptions, aero_cli};

#[tokio::main]
async fn main() {
    let result = aero_cli(TerminalOptions::new().with_prompt("$ "), None).await;
    if let Err(err) = result {
        println!("{err}");
    }
}
