use crate::imports::*;

#[derive(Default, Handler)]
#[help("Connect to a Kaspa network")]
pub struct Connect;

impl Connect {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let url = argv.first().cloned().or_else(|| ctx.wallet().settings().get(WalletSettings::Server));
        let network_type = ctx.wallet().network_id()?;
        let url = ctx.wallet().rpc_client().parse_url_with_network_type(url, network_type.into()).map_err(|e| e.to_string())?;
        let options = ConnectOptions { block_async_connect: true, strategy: ConnectStrategy::Fallback, url, timeout: None };
        ctx.wallet().rpc_client().connect(options).await.map_err(|e| e.to_string())?;
        Ok(())
    }
}
