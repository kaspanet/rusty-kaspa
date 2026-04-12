use keryx_cli_lib::keryx_cli;
use wasm_bindgen::prelude::*;
use workflow_terminal::Options;
use workflow_terminal::Result;

#[wasm_bindgen]
pub async fn load_kaspa_wallet_cli() -> Result<()> {
    let options = Options { ..Options::default() };
    keryx_cli(options, None).await?;
    Ok(())
}
