use crate::imports::*;
use kaspa_wrpc_client::parse::parse_host;

#[derive(Default, Handler)]
#[help("Set RPC server address")]
pub struct Server;

impl Server {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if let Some(url) = argv.first() {
            let Ok(_) = parse_host(url) else {
                tprintln!(ctx, "Invalid host: {url}");
                return Ok(());
            };

            ctx.wallet().settings().set(WalletSettings::Server, url).await?;
            tprintln!(ctx, "Setting RPC server to: {url}");
        } else {
            let server = ctx.wallet().settings().get(WalletSettings::Server).unwrap_or_else(|| "n/a".to_string());
            tprintln!(ctx, "Current RPC server is: {server}");
        }

        Ok(())
    }
}
