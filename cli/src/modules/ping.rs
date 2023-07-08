use crate::imports::*;

#[derive(Default, Handler)]
#[help("Ping the connected node")]
pub struct Ping;

impl Ping {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<WalletCli>()?;
        if ctx.wallet().ping().await {
            tprintln!(ctx, "ping ok");
        } else {
            terrorln!(ctx, "ping error");
        }
        Ok(())
    }
}
