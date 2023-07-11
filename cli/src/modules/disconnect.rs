use crate::imports::*;

#[derive(Default, Handler)]
#[help("Disconnects from the kaspa network")]
pub struct Disconnect;

impl Disconnect {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        ctx.wallet().rpc_client().shutdown().await?;
        Ok(())
    }
}
