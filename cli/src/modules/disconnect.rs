use crate::imports::*;

#[derive(Default, Handler)]
#[help("Disconnect from the kaspa network")]
pub struct Disconnect;

impl Disconnect {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        if let Some(wrpc_client) = ctx.wallet().wrpc_client().as_ref() {
            wrpc_client.shutdown().await?;
        } else {
            terrorln!(ctx, "Unable to disconnect from non-wRPC client");
        }
        Ok(())
    }
}
