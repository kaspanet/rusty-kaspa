use crate::imports::*;

#[derive(Default, Handler)]
#[help("Set RPC server address")]
pub struct Server;

impl Server {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if let Some(url) = argv.first() {
            ctx.wallet().settings().set(WalletSettings::Server, url).await?;
            tprintln!(ctx, "Setting RPC server to: {url}");
        } else {
            let server = ctx.wallet().settings().get(WalletSettings::Server).unwrap_or_else(|| "n/a".to_string());
            tprintln!(ctx, "Current RPC server is: {server}");
        }

        Ok(())
    }
}
