use aero_cli_lib::aero_cli;
use wasm_bindgen::prelude::*;
use workflow_terminal::Options;
use workflow_terminal::Result;

#[wasm_bindgen]
pub async fn load_aero_wallet_cli() -> Result<()> {
    let options = Options { ..Options::default() };
    aero_cli(options, None).await?;
    Ok(())
}
