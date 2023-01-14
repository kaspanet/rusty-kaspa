use kaspa_wallet_cli::kaspa_wallet_cli;
use wasm_bindgen::prelude::*;
use workflow_terminal::Result;

#[wasm_bindgen(start)]
pub async fn load_kaspa_wallet_cli() -> Result<()> {
    console_error_panic_hook::set_once();
    kaspa_wallet_cli().await?;
    Ok(())
}
