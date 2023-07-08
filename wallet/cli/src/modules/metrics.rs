use crate::imports::*;

#[derive(Default, Handler)]
#[help("Toggle network metrics monitoring")]
pub struct Metrics;

impl Metrics {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<WalletCli>()?;
        let response = ctx.wallet().rpc().get_metrics(true, true).await.map_err(|e| e.to_string())?;
        tprintln!(ctx, "{:#?}", response);
        Ok(())
    }
}
